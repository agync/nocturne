import type {
  BackupPhase,
  BackupProgress,
  BackupReport,
  BackupSummary,
  BackupsView,
  Holdings,
  LastBackup,
  PutAsideView,
  RestoreCheck,
  RestoredView,
} from "../bindings";
import { EngineError } from "../client";

/** The failures a scripted backup can end with, besides `invalid`, `busy` and `cancelled`. */
export type BackupFailure = "noRoom" | "cantWrite" | "notFound" | "damagedBackup" | "other";
/** The failures a scripted check or restore can end with. */
export type FileFailure = "notABackup" | "newerBackup" | "damagedBackup" | "noRoom" | "notFound";

export interface BackupScript {
  /** The next backup fails this way, once. `invalid` comes from a short passphrase, without a script. */
  fail?: BackupFailure | null;
  /** A backup that is already running, for a screen opened while one runs. */
  running?: BackupProgress | null;
}

export interface ProbeScript {
  /** The next probe refuses the file this way, once. */
  fail?: FileFailure | "other" | null;
}

export interface ProgressScript {
  /** Byte fractions reported one step apart before the work finishes. Omitted for no progress. */
  steps?: number[] | null;
  /** How long the final step stays on screen before the work finishes. */
  holdFinalMs?: number | null;
}

export interface CheckScript extends ProgressScript {
  /** The next check fails this way, once. A passphrase of "wrong passphrase" is refused first. */
  fail?: FileFailure | "wrongPassphrase" | "other" | null;
}

export interface RestoreScript extends ProgressScript {
  /** The next restore fails this way, once. */
  fail?: FileFailure | "wrongPassphrase" | "other" | null;
}

/**
 * What the mock does next. Set it from a test or the console: `engine.script.backup = { fail:
 * "noRoom" }`. A value is used once unless it is a list or an install state, which stands until
 * changed.
 */
export interface MockBackupsScript {
  backup: BackupScript;
  probe: ProbeScript;
  check: CheckScript;
  restore: RestoreScript;
  /** The next bring-back walks this progress before it finishes. */
  bringBack: ProgressScript;
  /** What this install holds now; `null` is an empty computer. Unset means it holds the seed. */
  here?: Holdings | null;
  /** The copies put aside, replacing the ones the mock has. */
  putAside?: PutAsideView[] | null;
  /** What the last restore brought in; `null` means nothing was restored. */
  restored?: RestoredView | null;
  interruptedBackup?: boolean | null;
  interruptedRestore?: boolean | null;
}

export interface Counts {
  memories: number;
  pages: number;
  originals: number;
}

interface BackupOptions {
  counts: () => Counts;
  now: () => number;
  onlineUsed: () => boolean;
  awayUsed: () => boolean;
  onProgress?: (progress: BackupProgress) => void;
}

/** How long each progress step takes. Short, so a screen never waits on the preview. */
const STEP_MS = 80;

/** The bytes a mock backup reports before any kept thing is counted. */
const BASE_BYTES = 4_300_000_000;

const isBackupFile = (path: string) => path.toLowerCase().endsWith(".nocturne");

const engineError = (code: string, detail: string) =>
  new EngineError(code === "other" ? "unknown" : code, detail);

/** One run's promise, and the way to stop it. */
interface Running {
  stop: (error: EngineError) => void;
}

/**
 * Backups, restores and their files, in memory. Everything is deterministic: a backup walks the
 * same byte fractions on the same timers, and every failure is one script assignment away.
 */
export class MockBackups {
  readonly script: MockBackupsScript = {
    backup: {},
    probe: {},
    check: {},
    restore: {},
    bringBack: {},
  };
  private last: LastBackup | null = null;
  private putAside: PutAsideView[] = [];
  private running: Running | null = null;
  private progress: BackupProgress | null = null;
  private interruptedBackup = false;
  private interruptedRestore = false;
  private restored: RestoredView | null = null;

  constructor(private readonly options: BackupOptions) {}

  estimate(): number {
    return BASE_BYTES + this.options.counts().memories * 512;
  }

  view(): BackupsView {
    return {
      last: this.last ? { ...this.last } : null,
      putAside: structuredClone(this.script.putAside ?? this.putAside),
      running: structuredClone(this.script.backup.running ?? this.progress),
      interruptedBackup: this.script.interruptedBackup ?? this.interruptedBackup,
      interruptedRestore: this.script.interruptedRestore ?? this.interruptedRestore,
    };
  }

  dismissNotices(): void {
    this.interruptedBackup = false;
    this.interruptedRestore = false;
    this.script.interruptedBackup = false;
    this.script.interruptedRestore = false;
  }

  async makeBackup(path: string, passphrase: string): Promise<BackupReport> {
    if ([...passphrase].length < 8) {
      throw new EngineError("invalid", "use at least 8 characters");
    }
    if (this.running) throw new EngineError("busy", "a backup is already running");
    const failure = this.script.backup.fail ?? null;
    this.script.backup.fail = null;
    this.script.backup.running = null;
    this.script.interruptedBackup = false;
    this.interruptedBackup = false;
    if (failure === "cantWrite") {
      throw new EngineError("cantWrite", "that folder does not let Nocturne save files");
    }
    if (failure === "notFound") {
      throw new EngineError("notFound", "the drive went away");
    }

    const counts = this.options.counts();
    const total = this.estimate();
    const report: BackupReport = { path, ...counts, bytes: total };

    return new Promise<BackupReport>((resolve, reject) => {
      const timers: Array<ReturnType<typeof setTimeout>> = [];
      let stopped = false;
      const finish = (error?: EngineError) => {
        if (stopped) return;
        stopped = true;
        for (const timer of timers) clearTimeout(timer);
        this.running = null;
        this.progress = null;
        if (error) {
          reject(error);
          return;
        }
        this.last = { at: new Date(this.options.now()).toISOString(), bytes: total };
        resolve({ ...report });
      };
      this.running = { stop: (error) => finish(error) };

      const steps: Array<{ phase: BackupPhase; fraction: number; fail?: EngineError }> = [
        {
          phase: "saving",
          fraction: 0.25,
          fail: failure === "noRoom" ? new EngineError("noRoom", "the drive filled up") : undefined,
        },
        {
          phase: "saving",
          fraction: 0.5,
          fail:
            failure === "other"
              ? new EngineError("unknown", "the backup did not finish")
              : undefined,
        },
        { phase: "saving", fraction: 0.75 },
        {
          phase: "checking",
          fraction: 0.5,
          fail:
            failure === "damagedBackup"
              ? new EngineError("damagedBackup", "the file did not read back")
              : undefined,
        },
        { phase: "checking", fraction: 0.9 },
        { phase: "checking", fraction: 1 },
      ];

      const next = (index: number) => {
        if (stopped) return;
        const step = steps[index];
        if (!step) {
          finish();
          return;
        }
        this.progress = {
          phase: step.phase,
          done: Math.round(total * step.fraction),
          total,
        };
        this.options.onProgress?.(structuredClone(this.progress));
        if (step.fail) {
          const error = step.fail;
          timers.push(setTimeout(() => finish(error), STEP_MS));
          return;
        }
        timers.push(setTimeout(() => next(index + 1), STEP_MS));
      };
      next(0);
    });
  }

  cancel(): void {
    this.running?.stop(new EngineError("cancelled", "stopped"));
  }

  /** Walks the scripted progress for a check or restore, or returns at once when none is set. */
  private walk(phase: BackupPhase, total: number, script: ProgressScript): Promise<void> {
    const steps = script.steps ?? null;
    if (!steps || steps.length === 0) return Promise.resolve();
    return new Promise<void>((resolve, reject) => {
      const timers: Array<ReturnType<typeof setTimeout>> = [];
      let stopped = false;
      const finish = (error?: EngineError) => {
        if (stopped) return;
        stopped = true;
        for (const timer of timers) clearTimeout(timer);
        this.running = null;
        this.progress = null;
        if (error) reject(error);
        else resolve();
      };
      this.running = { stop: (error) => finish(error) };
      let index = 0;
      const next = () => {
        if (stopped) return;
        const fraction = steps[index];
        index += 1;
        if (fraction === undefined) {
          finish();
          return;
        }
        this.progress = {
          phase,
          done: Math.round(total * fraction),
          total,
        };
        this.options.onProgress?.(structuredClone(this.progress));
        const wait = index >= steps.length ? STEP_MS + (script.holdFinalMs ?? 0) : STEP_MS;
        timers.push(setTimeout(next, wait));
      };
      next();
    });
  }

  async probeBackup(path: string): Promise<void> {
    const failure = this.script.probe.fail ?? null;
    this.script.probe.fail = null;
    if (failure) throw engineError(failure, "that file cannot be opened");
    if (!isBackupFile(path)) throw new EngineError("notABackup", "not a Nocturne backup");
  }

  async checkBackup(path: string, passphrase: string): Promise<RestoreCheck> {
    if (passphrase === "wrong passphrase") {
      throw new EngineError("wrongPassphrase", "that passphrase does not open this backup");
    }
    const failure = this.script.check.fail ?? null;
    const progress = { steps: this.script.check.steps, holdFinalMs: this.script.check.holdFinalMs };
    this.script.check.fail = null;
    this.script.check.steps = null;
    this.script.check.holdFinalMs = null;
    if (failure) {
      if (failure === "newerBackup") {
        throw new EngineError("newerBackup", "made by Nocturne 0.4.0");
      }
      throw engineError(failure, "that backup cannot be restored");
    }
    if (!isBackupFile(path)) throw new EngineError("notABackup", "not a Nocturne backup");
    const summary = this.summary();
    await this.walk("checking", summary.bytes, progress);
    return {
      summary,
      here: this.here(),
      putAside: (this.script.putAside ?? this.putAside)[0]?.at ?? null,
    };
  }

  /** What one of the mock's backup files holds. It has no passphrase or path of its own. */
  private summary(): BackupSummary {
    const counts = this.options.counts();
    return {
      madeAt: new Date(this.options.now() - 86_400_000).toISOString(),
      appVersion: "0.2.2",
      storeVersion: 1,
      memories: counts.memories,
      items: counts.originals,
      pages: counts.pages,
      originals: counts.originals,
      onlineUsed: this.options.onlineUsed(),
      awayUsed: this.options.awayUsed(),
      bytes: this.estimate(),
    };
  }

  async restoreBackup(path: string, passphrase: string, replace: boolean): Promise<void> {
    if (passphrase === "wrong passphrase") {
      throw new EngineError("wrongPassphrase", "that passphrase does not open this backup");
    }
    const failure = this.script.restore.fail ?? null;
    const progress = {
      steps: this.script.restore.steps,
      holdFinalMs: this.script.restore.holdFinalMs,
    };
    this.script.restore.fail = null;
    this.script.restore.steps = null;
    this.script.restore.holdFinalMs = null;
    if (failure) throw engineError(failure, "that backup cannot be restored");
    if (!isBackupFile(path)) throw new EngineError("notABackup", "not a Nocturne backup");
    const here = this.here();
    if (here && !replace) {
      throw new EngineError("invalid", "this install already holds things of yours");
    }
    const summary = this.summary();
    await this.walk("restoring", summary.bytes, progress);
    const at = new Date(this.options.now()).toISOString();
    if (here) {
      this.putAside.unshift({
        id: at,
        at,
        memories: here.memories,
        pages: here.pages,
        originals: here.originals,
        bytes: here.bytes,
      });
    }
    this.restored = {
      madeAt: summary.madeAt,
      memories: summary.memories,
      pages: summary.pages,
      originals: summary.originals,
      onlineUsed: summary.onlineUsed,
      awayUsed: summary.awayUsed,
      putAsideAt: here ? at : null,
    };
    this.interruptedRestore = false;
    this.script.interruptedRestore = false;
  }

  async restorePutAside(id: string): Promise<void> {
    const copies = this.script.putAside ?? this.putAside;
    const found = copies.find((copy) => copy.id === id);
    if (!found) throw new EngineError("notFound", "that copy is not there");
    const progress = {
      steps: this.script.bringBack.steps,
      holdFinalMs: this.script.bringBack.holdFinalMs,
    };
    this.script.bringBack.steps = null;
    this.script.bringBack.holdFinalMs = null;
    await this.walk("restoring", found.bytes > 0 ? found.bytes : this.estimate(), progress);

    const at = new Date(this.options.now()).toISOString();
    const here = this.here();
    const left = copies.filter((copy) => copy.id !== id);
    if (here) {
      left.unshift({
        id: at,
        at,
        memories: here.memories,
        pages: here.pages,
        originals: here.originals,
        bytes: here.bytes,
      });
    }
    if (this.script.putAside) this.script.putAside = left;
    else this.putAside = left;
    this.restored = {
      madeAt: found.at,
      memories: found.memories,
      pages: found.pages,
      originals: found.originals,
      onlineUsed: this.options.onlineUsed(),
      awayUsed: this.options.awayUsed(),
      putAsideAt: found.at,
    };
    this.interruptedRestore = false;
    this.script.interruptedRestore = false;
  }

  async deletePutAside(id: string): Promise<void> {
    const copies = this.script.putAside ?? this.putAside;
    if (!copies.some((copy) => copy.id === id)) {
      throw new EngineError("notFound", "that copy is not there");
    }
    const left = copies.filter((copy) => copy.id !== id);
    if (this.script.putAside) this.script.putAside = left;
    else this.putAside = left;
  }

  getRestored(): RestoredView | null {
    return structuredClone(
      this.script.restored !== undefined ? this.script.restored : this.restored,
    );
  }

  finishRestore(): void {
    this.restored = null;
    this.script.restored = null;
  }

  discardRestore(): void {
    this.restored = null;
    this.script.restored = null;
  }

  private here(): Holdings | null {
    if (this.script.here !== undefined) return structuredClone(this.script.here);
    const counts = this.options.counts();
    return {
      ...counts,
      lastKept: new Date(this.options.now() - 3_600_000).toISOString(),
      keptAfterBackup: 0,
      bytes: this.estimate(),
    };
  }
}
