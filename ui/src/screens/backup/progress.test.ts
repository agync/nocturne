import { describe, expect, it } from "vitest";
import type { BackupProgress } from "../../engine/bindings";
import { etaSeconds, progressDetail } from "./progress";

const progress = (done: number, total = 1000): BackupProgress => ({
  phase: "saving",
  done,
  total,
});

describe("the time left in a backup", () => {
  it("says nothing until a second of movement can be trusted", () => {
    const first = { phase: "saving" as const, done: 100, at: 0 };
    expect(etaSeconds(progress(600), first, 500)).toBeNull();
    expect(etaSeconds(progress(600), first, 1000)).toBe(0.8);
  });

  it("says nothing when nothing moved, or it is already done", () => {
    const first = { phase: "saving" as const, done: 100, at: 0 };
    expect(etaSeconds(progress(100), first, 2000)).toBeNull();
    expect(etaSeconds(progress(1000), first, 5000)).toBeNull();
  });

  it("measures from the first sample, so a slowdown is included", () => {
    const first = { phase: "checking" as const, done: 0, at: 0 };
    expect(etaSeconds(progress(250, 1000), first, 2000)).toBe(6);
  });
});

describe("the words under a progress bar", () => {
  it("gives bytes and the estimate only when there is one", () => {
    expect(progressDetail(progress(1_600_000_000, 4_300_000_000), null)).toBe(
      "1.6\u00a0GB of 4.3\u00a0GB",
    );
    expect(progressDetail(progress(1_600_000_000, 4_300_000_000), 240)).toBe(
      "1.6\u00a0GB of 4.3\u00a0GB · about 4 min left",
    );
  });
});
