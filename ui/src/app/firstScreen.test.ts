import { describe, expect, it } from "vitest";
import type { SetupView } from "../engine/bindings";
import { firstScreen } from "./firstScreen";

const base: SetupView = {
  phase: "done",
  detected: "small",
  chosen: "small",
  ramGib: 16,
  freeDiskBytes: 1e11,
  options: [],
  downloadedBytes: 0,
  totalBytes: 0,
  etaSeconds: null,
  paused: null,
  error: null,
  neededBytes: null,
};

describe("firstScreen", () => {
  it("opens setup until a size has been chosen, whatever the phase", () => {
    for (const phase of ["notStarted", "checking", "choosing", "failed"] as const) {
      expect(firstScreen({ ...base, phase, chosen: null })).toBe("/setup");
    }
  });

  it("opens today's page once a size was chosen, even while it downloads or failed", () => {
    for (const phase of ["notStarted", "downloading", "paused", "installing", "failed"] as const) {
      expect(firstScreen({ ...base, phase, chosen: "small" })).toBe("/today");
    }
    expect(firstScreen(base)).toBe("/today");
  });
});
