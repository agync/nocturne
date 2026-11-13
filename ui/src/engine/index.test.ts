import { afterEach, describe, expect, it, vi } from "vitest";

const TauriEngine = vi.fn();
vi.mock("./tauri", () => ({ TauriEngine }));

import { createEngine } from "./index";

afterEach(() => {
  vi.unstubAllEnvs();
  TauriEngine.mockReset();
  window.location.hash = "";
});

describe("createEngine", () => {
  it("listens in a window that is not quick capture, whatever route it shows", async () => {
    vi.stubEnv("VITE_ENGINE", "tauri");
    window.location.hash = "#/quick-capture";
    await createEngine({ inQuickCapture: () => false });
    expect(TauriEngine).toHaveBeenCalledWith({ listen: true });
  });

  it("does not listen in the quick capture window, whatever route it shows", async () => {
    vi.stubEnv("VITE_ENGINE", "tauri");
    window.location.hash = "#/today";
    await createEngine({ inQuickCapture: () => true });
    expect(TauriEngine).toHaveBeenCalledWith({ listen: false });
  });
});
