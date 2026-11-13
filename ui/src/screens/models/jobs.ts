import type { CopyId } from "../../copy/t";
import type { Job, JobView, ModelsView, Tier } from "../../engine/bindings";

export const JOB_ORDER: Job[] = ["answering", "recordings", "aloud", "finding"];

export const JOB_TITLE = {
  answering: "screen.models.answering.title",
  recordings: "screen.models.recordings.title",
  aloud: "screen.models.aloud.title",
  finding: "screen.models.finding.title",
} as const satisfies Record<Job, CopyId>;

export const JOB_HINT = {
  answering: "models.job.answering.hint",
  recordings: "models.job.recordings.hint",
  aloud: "models.job.aloud.hint",
  finding: "models.job.finding.hint",
} as const satisfies Record<Job, CopyId>;

export const JOB_SHORT = {
  answering: "models.job.answering.short",
  recordings: "models.job.recordings.short",
  aloud: "models.job.aloud.short",
  finding: "models.job.finding.short",
} as const satisfies Record<Job, CopyId>;

export const JOB_DOING = {
  answering: "models.job.answering.doing",
  recordings: "models.job.recordings.doing",
  aloud: "models.job.aloud.doing",
  finding: "models.job.finding.doing",
} as const satisfies Record<Job, CopyId>;

export const PICK_TITLE = {
  answering: "models.pick.title.answering",
  recordings: "models.pick.title.recordings",
  aloud: "models.pick.title.aloud",
  finding: "models.pick.title.finding",
} as const satisfies Record<Job, CopyId>;

export const PICK_INTRO = {
  answering: "models.pick.intro.answering",
  recordings: "models.pick.intro.recordings",
  aloud: "models.pick.intro.aloud",
  finding: "models.pick.intro.finding",
} as const satisfies Record<Job, CopyId>;

export const PICK_SMALLER = {
  answering: "models.pick.smaller.answering",
  recordings: "models.pick.smaller.recordings",
  aloud: "models.pick.smaller.aloud",
  finding: "models.pick.smaller.finding",
} as const satisfies Record<Job, CopyId>;

export const TIER_NAME = {
  large: "setup.tier.large",
  standard: "setup.tier.standard",
  small: "setup.tier.small",
  tiny: "setup.tier.tiny",
  unsupported: "setup.tier.tiny",
} as const satisfies Record<Tier, CopyId>;

export const isJob = (value: string | undefined): value is Job =>
  value === "answering" || value === "recordings" || value === "aloud" || value === "finding";

/** "8.1 GB", "0.64 GB", "220 GB": gigabytes throughout, as the designs show them. */
export function gb(bytes: number): string {
  const value = bytes / 1e9;
  const digits = value >= 100 ? 0 : value >= 1 ? 1 : 2;
  return `${new Intl.NumberFormat("en-GB", { maximumFractionDigits: digits }).format(value)} GB`;
}

export const jobOf = (view: ModelsView, job: Job): JobView | undefined =>
  view.jobs.find((j) => j.job === job);

/** The jobs whose model is not the tier's own, as far as the view says (the recommended flag). */
export const changedJobs = (view: ModelsView): Job[] =>
  JOB_ORDER.filter((job) => jobOf(view, job)?.recommended === false);
