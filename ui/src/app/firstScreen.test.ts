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
      expect(firstScreen({ ...base, phase, chosen: null }, "gettingReady")).toBe("/setup");
      expect(firstScreen({ ...base, phase, chosen: null }, null)).toBe("/setup");
    }
  });

  it("opens setup again while a chosen first size is on its way", () => {
    for (const phase of ["choosing", "downloading", "paused", "installing"] as const) {
      expect(firstScreen({ ...base, phase, chosen: "small" }, "gettingReady")).toBe("/setup");
    }
  });

  it("opens today's page while a change from an earlier size is on its way", () => {
    for (const phase of ["downloading", "paused", "installing"] as const) {
      expect(firstScreen({ ...base, phase, chosen: "small" }, "resting")).toBe("/today");
    }
  });

  it("opens today's page once set up, while the engine starts or after it gave up", () => {
    for (const phase of ["notStarted", "checking", "failed"] as const) {
      expect(firstScreen({ ...base, phase, chosen: "small" }, "gettingReady")).toBe("/today");
    }
    expect(firstScreen(base, "resting")).toBe("/today");
  });
});
