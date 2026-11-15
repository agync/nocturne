import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NpuCause, NpuView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { npuDetails, npuFixable } from "./Advanced";

const NONE: NpuView = {
  present: false,
  used: false,
  model: null,
  causes: [],
  detail: null,
  mind: null,
};
const blocked = (causes: NpuCause[], detail: string | null = null): NpuView => ({
  present: true,
  used: false,
  model: null,
  causes,
  detail,
  mind: null,
});

beforeEach(() => {
  setViewport(1280);
  localStorage.clear();
});
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

function engineWith(npu: NpuView) {
  const engine = createMockEngine();
  const real = engine.getAdvanced.bind(engine);
  vi.spyOn(engine, "getAdvanced").mockImplementation(async () => ({ ...(await real()), npu }));
  return engine;
}

describe("the NPU in the advanced details", () => {
  it("says there is none, or what it reads", () => {
    expect(npuDetails(NONE)).toEqual([["NPU", "none found"]]);
    expect(
      npuDetails({ ...NONE, present: true, used: true, model: "whisper-v3-turbo-FLM" }),
    ).toEqual([["NPU", "reads recordings · whisper-v3-turbo-FLM"]]);
  });

  it("says when it also reads what you keep, or reads and answers", () => {
    const used = { ...NONE, present: true, used: true, model: "whisper-v3-turbo-FLM" };
    const mind = (answers: boolean) => ({ model: "gemma4-it-e4b-FLM", answers });
    expect(npuDetails({ ...used, mind: mind(false) })).toEqual([
      ["NPU", "reads recordings and what you keep · whisper-v3-turbo-FLM, gemma4-it-e4b-FLM"],
    ]);
    expect(npuDetails({ ...used, mind: mind(true) })).toEqual([
      ["NPU", "reads, answers and reads recordings · whisper-v3-turbo-FLM, gemma4-it-e4b-FLM"],
    ]);
  });

  it("gives every cause a reason and every fixable one its steps", () => {
    const cases: Array<[NpuCause, string, string | null]> = [
      ["driverMissing", "the amdxdna kernel driver is not loaded", "Use Linux 7.0 or later"],
      ["driverOld", "the amdxdna driver is too old", "Update the amdxdna driver"],
      ["kernelOld", "the kernel is too old for the NPU", "Update to Linux 7.0 or later"],
      ["firmwareOld", "the NPU firmware is too old", "Update linux-firmware"],
      ["memoryLock", "the memory lock limit (ulimit -l) is too low", "Set memlock to unlimited"],
      ["notSupported", "the engine does not support this NPU", null],
      ["tooLittleMemory", "its speech model does not fit the memory budget", null],
      ["notFetched", "its runtime or speech model is not downloaded", null],
      ["chosen", "a speech model was chosen for reading recordings", null],
      ["failed", "it failed; recordings are read on the GPU or CPU", null],
    ];
    for (const [cause, reason, step] of cases) {
      const rows = npuDetails(blocked([cause]));
      expect(rows[0]?.[0], cause).toBe("NPU");
      expect(rows[0]?.[1], cause).toContain(`not used · ${reason}`);
      const fix = rows.find(([label]) => label === "NPU, to fix");
      if (step) {
        expect(fix?.[1], cause).toContain(step);
        expect(fix?.[1], cause).toContain("nocturne.is/docs/guide/npu.html");
      } else {
        expect(fix, cause).toBeUndefined();
      }
      expect(npuFixable(blocked([cause])), cause).toBe(step !== null);
    }
  });

  it("keeps every reason of one refusal and the engine's own words", () => {
    const rows = npuDetails(
      blocked(
        ["kernelOld", "firmwareOld"],
        "FLM NPU validation failed: Kernel version is incompatible.",
      ),
    );
    expect(rows).toEqual([
      ["NPU", "not used · the kernel is too old for the NPU; the NPU firmware is too old"],
      ["NPU, engine said", "FLM NPU validation failed: Kernel version is incompatible."],
      [
        "NPU, to fix",
        expect.stringMatching(
          /^Update to Linux 7\.0 or later, then restart\. Update linux-firmware/,
        ),
      ],
    ]);
  });

  it("shows the rows on the screen", async () => {
    renderApp({ hash: "#/settings/advanced", engine: engineWith(blocked(["memoryLock"])) });
    expect(
      await screen.findByText("not used · the memory lock limit (ulimit -l) is too low"),
    ).toBeInTheDocument();
    expect(screen.getByText(/standard · 32 GB RAM · .* · NPU$/)).toBeInTheDocument();
  });
});

describe("the quiet note in Settings", () => {
  const openSettings = async (npu: NpuView) => {
    renderApp({ hash: "#/settings", engine: engineWith(npu) });
    await screen.findByRole("heading", { level: 1, name: "Settings" });
  };
  const NOTE = "This computer has a part that could make Nocturne faster, but it isn’t set up.";

  it("appears only when the person can set the part up", async () => {
    await openSettings(blocked(["memoryLock"]));
    expect(await screen.findByText(NOTE)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "How to set it up" })).toHaveAttribute(
      "href",
      "#/settings/advanced",
    );
  });

  it("stays away when nothing can be done, nothing is wrong or there is no such part", async () => {
    for (const npu of [
      NONE,
      blocked(["notSupported"]),
      blocked(["tooLittleMemory"]),
      blocked(["failed"], "flm serve exited"),
      { ...NONE, present: true, used: true, model: "whisper-v3-turbo-FLM" },
    ]) {
      const { unmount } = renderApp({ hash: "#/settings", engine: engineWith(npu) });
      await screen.findByRole("combobox", { name: "Voice, name" });
      expect(screen.queryByText(NOTE), JSON.stringify(npu)).not.toBeInTheDocument();
      unmount();
    }
  });

  it("can be hidden, and stays hidden", async () => {
    await openSettings(blocked(["driverMissing"]));
    await userEvent.click(await screen.findByRole("button", { name: "Hide this" }));
    await waitFor(() => expect(screen.queryByText(NOTE)).not.toBeInTheDocument());
    expect(localStorage.getItem("nocturne.fasterNote.hidden")).toBe("1");
  });
});
