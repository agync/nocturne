import type { LettingGo } from "../engine/bindings";

/** Days without use before a memory fades, and before it is let go, for each letting-go pace. */
const PACE: Record<LettingGo, { fade: number; letGo: number }> = {
  gentle: { fade: 45, letGo: 120 },
  normal: { fade: 30, letGo: 90 },
  brisk: { fade: 14, letGo: 45 },
};

export function paceDays(pace: LettingGo | undefined): { fade: number; letGo: number } {
  return PACE[pace ?? "normal"];
}
