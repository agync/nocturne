import type { ChangePreview, Job, JobView, ModelChoice, ModelsView, Tier } from "../bindings";
import { EngineError } from "../client";

const unavailable = () => new EngineError("unavailable", "choosing models is not available yet");

/** The Standard tier's model for each job, as the mock's computer has it. */
const STANDARD: JobView[] = [
  { job: "answering", model: "Qwen3.5-9B-GGUF", name: "Qwen3.5-9B-GGUF", sizeBytes: 6.88e9 },
  { job: "recordings", model: "Whisper-Small", name: "Whisper-Small", sizeBytes: 0.5e9 },
  { job: "aloud", model: "kokoro-v1", name: "kokoro-v1", sizeBytes: 0.354e9 },
  {
    job: "finding",
    model: "Qwen3-Embedding-0.6B-GGUF",
    name: "Qwen3-Embedding-0.6B-GGUF",
    sizeBytes: 0.64e9,
  },
].map((j) => ({ ...j, job: j.job as Job, memoryBytes: 0, recommended: true, tested: true }));

/** Choosing models in the mock: the tier's preset. The rest is not available yet. */
export class ModelsSim {
  constructor(private readonly tier: Tier = "standard") {}

  view(): ModelsView {
    return {
      preset: this.tier,
      custom: false,
      ramBytes: 0,
      budgetBytes: 0,
      busiestBytes: 0,
      jobs: STANDARD.map((j) => ({ ...j })),
      change: null,
      reindex: null,
      old: null,
      attention: false,
    };
  }

  async choices(_job: Job): Promise<ModelChoice[]> {
    throw unavailable();
  }

  async preview(_job: Job, _model: string): Promise<ChangePreview> {
    throw unavailable();
  }

  async change(_job: Job, _model: string, _allowDownload: boolean): Promise<ModelsView> {
    throw unavailable();
  }

  async step(): Promise<ModelsView> {
    throw unavailable();
  }
}
