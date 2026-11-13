import type {
  ChangePreview,
  ChangeView,
  Fit,
  FitReason,
  Job,
  JobView,
  ModelChoice,
  ModelsView,
  OldModelView,
  ReindexView,
  Tier,
} from "../bindings";
import { EngineError } from "../client";

const GB = 1e9;
const MEMORIES = 3610;
const FREE_NOW = 220 * GB;

/** How long a download or a re-read the person starts takes in the mock, so a test can watch it. */
const RUN_MS = 4000;

/** One model the mock's computer knows about. Sizes are decimal gigabytes, as the screens show them. */
interface Entry {
  id: string;
  job: Job;
  size: number;
  memory: number;
  onDisk: boolean;
  tested: boolean;
  fit: Fit;
  reason: FitReason | null;
  smaller: boolean;
  recommended?: boolean;
}

const e = (
  job: Job,
  id: string,
  size: number,
  memory: number,
  rest: Partial<Omit<Entry, "id" | "job" | "size" | "memory">> = {},
): Entry => ({
  id,
  job,
  size: size * GB,
  memory: memory * GB,
  onDisk: false,
  tested: false,
  fit: "fits",
  reason: null,
  smaller: false,
  ...rest,
});

/** What the mock's computer can run, as the designs list it. */
const CATALOGUE: Entry[] = [
  e("answering", "Qwen3.5-9B-GGUF", 6.9, 8.1, { onDisk: true, tested: true, recommended: true }),
  e("answering", "Qwen3.5-4B-MTP", 3.7, 4.9, { onDisk: true, tested: true, smaller: true }),
  e("answering", "Qwen3-VL-8B", 6.2, 7.4),
  e("answering", "Gemma-4-E4B", 6.0, 7.2),
  e("answering", "Qwen3.5-2B", 2.0, 3.2, { tested: true, smaller: true }),
  e("answering", "MiniCPM-V-4.6", 1.6, 2.8, {
    smaller: true,
    fit: "cantUse",
    reason: { type: "notAvailable" },
  }),
  e("answering", "Gemma-4-26B-A4B", 18.1, 19.5, {
    fit: "wontFit",
    reason: { type: "memory", limiter: "answering" },
  }),
  e("answering", "Qwen3.6-35B-A3B-MTP", 23.8, 25.5, {
    tested: true,
    fit: "wontFit",
    reason: { type: "memory", limiter: null },
  }),
  e("recordings", "Whisper-Small", 0.49, 0.8, { onDisk: true, tested: true, recommended: true }),
  e("recordings", "Whisper-Large-v3-Turbo", 1.6, 2.1, { onDisk: true, tested: true }),
  e("recordings", "Whisper-Tiny", 0.08, 0.3, { tested: true, smaller: true }),
  e("recordings", "Whisper-Base", 0.15, 0.4, { tested: true, smaller: true }),
  e("recordings", "Whisper-Medium", 1.5, 2.0, { tested: true }),
  e("recordings", "Whisper-Large-v3", 3.1, 3.8, { tested: true }),
  e("aloud", "kokoro-v1", 0.35, 3.5, { onDisk: true, tested: true, recommended: true }),
  e("finding", "Qwen3-Embedding-0.6B-GGUF", 0.64, 1.7, {
    onDisk: true,
    tested: true,
    recommended: true,
  }),
  e("finding", "nomic-embed-text-v1", 0.08, 0.4, { smaller: true }),
  e("finding", "nomic-embed-text-v2-moe", 0.51, 1.2),
  e("finding", "Qwen3-Embedding-4B", 4.3, 5.0, { fit: "tight" }),
  e("finding", "Qwen3-Embedding-8B", 8.1, 9.0, {
    fit: "wontFit",
    reason: { type: "needsOtherJob", job: "answering" },
  }),
];

const PRESET: Record<Job, string> = {
  answering: "Qwen3.5-9B-GGUF",
  recordings: "Whisper-Small",
  aloud: "kokoro-v1",
  finding: "Qwen3-Embedding-0.6B-GGUF",
};

const JOBS: Job[] = ["answering", "recordings", "aloud", "finding"];

/**
 * Where the mock starts, from `?mockModels=<name>`: `standard`, `custom`, `attention` (a download
 * that stopped), `downloading`, `paused`, `resumed`, `reindex`, `reindexDone`, `reindexCancelled`,
 * `oldKeep`, `oldRemoved`, `tiny`.
 */
export type ModelsScenario =
  | "standard"
  | "custom"
  | "attention"
  | "downloading"
  | "paused"
  | "resumed"
  | "reindex"
  | "reindexDone"
  | "reindexCancelled"
  | "oldKeep"
  | "oldRemoved"
  | "tiny";

function scenarioFromAddress(): ModelsScenario {
  if (typeof location === "undefined") return "standard";
  return (new URLSearchParams(location.search).get("mockModels") as ModelsScenario) ?? "standard";
}

interface Run {
  job: Job;
  model: string;
  from: string;
  total: number;
  received: number;
  paused: boolean;
  failed: boolean;
  resumed: boolean;
  /** When it last started moving, and what it had by then: progress is computed from these. */
  startedAt: number | null;
  base: number;
  /** A scripted scenario stands still; one the person starts moves with the clock. */
  live: boolean;
}

interface Reread {
  model: string;
  from: string;
  done: number;
  total: number;
  phase: "running" | "done" | "cancelled";
  startedAt: number | null;
  live: boolean;
}

/** Choosing models in the mock: the tier's preset, a change with its download, a re-read, the old model. */
export class ModelsSim {
  private tier: Tier;
  private picks: Record<Job, string> = { ...PRESET };
  private custom = false;
  private run: Run | null = null;
  private reread: Reread | null = null;
  private old: OldModelView | null = null;
  private readonly onDisk = new Set(CATALOGUE.filter((m) => m.onDisk).map((m) => m.id));

  constructor(
    tier: Tier = "standard",
    scenario: ModelsScenario = scenarioFromAddress(),
    private readonly now: () => number = () => Date.now(),
  ) {
    this.tier = scenario === "tiny" ? "tiny" : tier;
    this.script(scenario);
  }

  private script(scenario: ModelsScenario) {
    const answering = PRESET.answering;
    const moving = (over: Partial<Run>): Run => ({
      job: "answering",
      model: "Qwen3-VL-8B",
      from: answering,
      total: 6.2 * GB,
      received: 2.1 * GB,
      paused: false,
      failed: false,
      resumed: false,
      startedAt: null,
      base: 2.1 * GB,
      live: false,
      ...over,
    });
    switch (scenario) {
      case "custom":
        this.onDisk.add("Qwen3-VL-8B");
        this.switchTo("answering", "Qwen3-VL-8B");
        this.old = null;
        break;
      case "attention":
        this.run = moving({ failed: true });
        break;
      case "downloading":
        this.run = moving({});
        break;
      case "paused":
        this.run = moving({ paused: true });
        break;
      case "resumed":
        this.run = moving({ resumed: true });
        break;
      case "reindex":
      case "reindexDone":
      case "reindexCancelled": {
        this.onDisk.add("Qwen3-Embedding-4B");
        const phase =
          scenario === "reindex" ? "running" : scenario === "reindexDone" ? "done" : "cancelled";
        this.reread = {
          model: "Qwen3-Embedding-4B",
          from: PRESET.finding,
          done: scenario === "reindex" ? 1240 : MEMORIES,
          total: MEMORIES,
          phase,
          startedAt: null,
          live: false,
        };
        if (scenario === "reindexDone") {
          this.picks.finding = "Qwen3-Embedding-4B";
          this.custom = true;
          this.old = { job: "finding", model: PRESET.finding, bytes: 0.64 * GB, removed: false };
        }
        break;
      }
      case "oldKeep":
        this.onDisk.add("Qwen3-VL-8B");
        this.switchTo("answering", "Qwen3-VL-8B");
        break;
      case "oldRemoved":
        this.onDisk.add("Qwen3-VL-8B");
        this.switchTo("answering", "Qwen3-VL-8B");
        if (this.old) this.old = { ...this.old, removed: true };
        this.onDisk.delete(answering);
        break;
      default:
        break;
    }
  }

  private entry(id: string): Entry {
    const found = CATALOGUE.find((m) => m.id === id);
    if (!found) throw new EngineError("notFound", `no such choice: ${id}`);
    return found;
  }

  private switchTo(job: Job, model: string) {
    const before = this.entry(this.picks[job]);
    this.picks[job] = model;
    this.custom = JOBS.some((j) => this.picks[j] !== PRESET[j]);
    this.old = { job, model: before.id, bytes: before.size, removed: false };
  }

  /** Moves a running download or re-read along by the clock, and lands it when it is whole. */
  private advance() {
    const now = this.now();
    const run = this.run;
    if (run?.live && !run.paused && !run.failed && run.startedAt !== null) {
      const share = Math.min(1, (now - run.startedAt) / RUN_MS);
      run.received = Math.min(run.total, run.base + (run.total - run.base) * share);
      if (share >= 1) this.land(run);
    }
    const reread = this.reread;
    if (reread?.live && reread.phase === "running" && reread.startedAt !== null) {
      const share = Math.min(1, (now - reread.startedAt) / RUN_MS);
      reread.done = Math.round(reread.total * share);
      if (share >= 1) this.finishReread(reread);
    }
  }

  private land(run: Run) {
    this.onDisk.add(run.model);
    this.run = null;
    if (run.job === "finding") {
      this.reread = {
        model: run.model,
        from: run.from,
        done: 0,
        total: MEMORIES,
        phase: "running",
        startedAt: this.now(),
        live: true,
      };
    } else {
      this.switchTo(run.job, run.model);
    }
  }

  private finishReread(reread: Reread) {
    reread.phase = "done";
    reread.done = reread.total;
    this.switchTo("finding", reread.model);
  }

  private jobView(job: Job): JobView {
    const m = this.entry(this.picks[job]);
    return {
      job,
      model: m.id,
      name: m.id,
      sizeBytes: m.size,
      memoryBytes: m.memory,
      recommended: this.picks[job] === PRESET[job],
      tested: m.tested,
    };
  }

  view(): ModelsView {
    this.advance();
    const jobs = JOBS.map((j) => this.jobView(j));
    const mem = (j: Job) => jobs.find((v) => v.job === j)?.memoryBytes ?? 0;
    const run = this.run;
    let change: ChangeView | null = null;
    if (run) {
      change = {
        job: run.job,
        model: run.model,
        from: run.from,
        resumed: run.resumed,
        phase: run.failed
          ? { type: "failed", received: run.received, total: run.total }
          : {
              type: "downloading",
              received: run.received,
              total: run.total,
              paused: run.paused,
            },
      };
    }
    const reread = this.reread;
    const reindex: ReindexView | null = reread
      ? {
          model: reread.model,
          from: reread.from,
          done: reread.done,
          total: reread.total,
          etaSeconds:
            reread.phase === "running"
              ? Math.round(((reread.total - reread.done) / reread.total) * 25 * 60)
              : null,
          phase: reread.phase,
        }
      : null;
    return {
      preset: this.tier,
      custom: this.custom,
      ramBytes: 32 * GB,
      budgetBytes: 19.3 * GB,
      busiestBytes: mem("answering") + mem("finding") + mem("recordings"),
      jobs,
      change,
      reindex,
      old: this.old ? { ...this.old } : null,
      attention: run?.failed ?? false,
    };
  }

  async choices(job: Job): Promise<ModelChoice[]> {
    await Promise.resolve();
    return CATALOGUE.filter((m) => m.job === job).map((m) => ({
      id: m.id,
      name: m.id,
      job,
      sizeBytes: m.size,
      memoryBytes: m.memory,
      onDisk: this.onDisk.has(m.id),
      current: this.picks[job] === m.id,
      recommended: m.recommended ?? false,
      tested: m.tested,
      fit: m.fit,
      reason: m.reason,
      smaller: m.smaller,
    }));
  }

  async preview(job: Job, model: string): Promise<ChangePreview> {
    await Promise.resolve();
    const next = this.entry(model);
    if (next.job !== job) throw new EngineError("invalid", "that choice is for another job");
    if (next.fit === "wontFit" || next.fit === "cantUse") {
      throw new EngineError("invalid", "this computer cannot use that choice");
    }
    const now = this.entry(this.picks[job]);
    const download = this.onDisk.has(model) ? 0 : next.size;
    return {
      job,
      model,
      from: now.id,
      downloadBytes: download,
      freeNow: FREE_NOW,
      freeAfter: FREE_NOW - download,
      memoryNow: now.memory,
      memoryAfter: next.memory,
      fit: next.fit,
      smaller: next.smaller,
      tested: next.tested,
      reindexItems: job === "finding" ? MEMORIES : null,
      reindexEtaSeconds: job === "finding" ? 25 * 60 : null,
      newOnly: job === "recordings",
    };
  }

  async change(job: Job, model: string, allowDownload: boolean): Promise<ModelsView> {
    await Promise.resolve();
    const next = this.entry(model);
    if (next.job !== job) throw new EngineError("invalid", "that choice is for another job");
    if (this.run) throw new EngineError("busy", "a change is already under way");
    if (next.fit === "wontFit" || next.fit === "cantUse") {
      throw new EngineError("invalid", "this computer cannot use that choice");
    }
    const present = this.onDisk.has(model);
    if (!present && !allowDownload) {
      throw new EngineError("needsConsent", "getting this needs the person's yes");
    }
    if (present && job === "finding") {
      this.reread = {
        model,
        from: this.picks.finding,
        done: 0,
        total: MEMORIES,
        phase: "running",
        startedAt: this.now(),
        live: true,
      };
    } else if (present) {
      this.switchTo(job, model);
    } else {
      this.run = {
        job,
        model,
        from: this.picks[job],
        total: next.size,
        received: 0,
        paused: false,
        failed: false,
        resumed: false,
        startedAt: this.now(),
        base: 0,
        live: true,
      };
    }
    return this.view();
  }

  async pause(): Promise<ModelsView> {
    await Promise.resolve();
    this.advance();
    const run = this.run;
    if (run && !run.failed) {
      run.paused = true;
      run.startedAt = null;
      run.base = run.received;
    }
    return this.view();
  }

  async resume(): Promise<ModelsView> {
    await Promise.resolve();
    const run = this.run;
    if (run) {
      run.paused = false;
      run.failed = false;
      run.live = true;
      run.startedAt = this.now();
      run.base = run.received;
    }
    return this.view();
  }

  /** Stops the download and removes what had come; the current model was never touched. */
  async cancel(): Promise<ModelsView> {
    await Promise.resolve();
    this.run = null;
    return this.view();
  }

  async cancelReindex(): Promise<ModelsView> {
    await Promise.resolve();
    if (this.reread?.phase === "running") {
      this.reread = { ...this.reread, phase: "cancelled", startedAt: null, live: false };
    }
    return this.view();
  }

  async keepOld(): Promise<ModelsView> {
    await Promise.resolve();
    this.old = null;
    this.reread = this.reread?.phase === "done" ? null : this.reread;
    return this.view();
  }

  async removeOld(): Promise<ModelsView> {
    await Promise.resolve();
    if (this.old && !this.old.removed) {
      this.onDisk.delete(this.old.model);
      this.old = { ...this.old, removed: true };
    }
    return this.view();
  }

  async reset(): Promise<ModelsView> {
    await Promise.resolve();
    this.picks = { ...PRESET };
    this.custom = false;
    this.old = null;
    return this.view();
  }

  /**
   * Lets a running download or re-read finish at once. The mock engine routes pause, resume,
   * cancel, keep, remove and reset here too, so it can only say "carry on"; the named methods above
   * are what each of those does.
   */
  async step(): Promise<ModelsView> {
    await Promise.resolve();
    if (this.run?.live) this.land(this.run);
    if (this.reread?.phase === "running") this.finishReread(this.reread);
    return this.view();
  }
}
