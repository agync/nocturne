import { addDays } from "../../copy/format";
import type { PhoneSources, ReadingKind } from "../../engine/bindings";

/** The order of the tabs, and the address of each kind. */
export const KIND_SLUGS = [
  ["sleep", "sleep"],
  ["heartRate", "heart-rate"],
  ["restingHeartRate", "resting-heart-rate"],
  ["bloodPressure", "blood-pressure"],
  ["steps", "steps"],
  ["workout", "workouts"],
  ["weight", "weight"],
] as const satisfies ReadonlyArray<readonly [ReadingKind, string]>;

export const slugOf = (kind: ReadingKind): string =>
  KIND_SLUGS.find(([k]) => k === kind)?.[1] ?? "sleep";

export function kindOfSlug(slug: string | undefined): ReadingKind | null {
  if (slug === undefined) return "sleep";
  return KIND_SLUGS.find(([, s]) => s === slug)?.[0] ?? null;
}

/** The phones that bring health. */
export const healthPhones = (phones: readonly PhoneSources[]): PhoneSources[] =>
  phones.filter((p) => p.health !== null);

/** The kinds any phone brings, in tab order. */
export function broughtKinds(phones: readonly PhoneSources[]): ReadingKind[] {
  const brought = new Set(healthPhones(phones).flatMap((p) => p.health?.kinds ?? []));
  return KIND_SLUGS.map(([k]) => k).filter((k) => brought.has(k));
}

/** The 30 days, 90 days or the year so far, as the first day shown. */
export type Range = "30" | "90" | "year";

export function rangeFrom(range: Range, today: string): string {
  if (range === "year") return `${today.slice(0, 4)}-01-01`;
  return addDays(today, -(Number(range) - 1));
}
