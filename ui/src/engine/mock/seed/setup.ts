import type { AdvancedView, SetupView } from "../../bindings";

const BASE: SetupView = {
  phase: "done",
  detected: "standard",
  chosen: "standard",
  ramGib: 32,
  freeDiskBytes: 220e9,
  options: [
    { tier: "large", downloadBytes: 26e9, minRamGib: 60, fits: false },
    { tier: "standard", downloadBytes: 8.4e9, minRamGib: 28, fits: true },
    { tier: "small", downloadBytes: 4.8e9, minRamGib: 14, fits: true },
  ],
  downloadedBytes: 8.4e9,
  totalBytes: 8.4e9,
  etaSeconds: null,
  paused: null,
  error: null,
  neededBytes: null,
};

const downloading = (over: Partial<SetupView>): SetupView => ({
  ...BASE,
  phase: "downloading",
  downloadedBytes: 4.9e9,
  etaSeconds: 1020,
  ...over,
});

/** The states of getting ready, for design review and tests: `?mockSetup=<name>` before the `#`. */
export const SETUP_SCENARIOS: Record<string, SetupView> = {
  done: BASE,
  notStarted: { ...BASE, phase: "notStarted", detected: null, chosen: null, downloadedBytes: 0 },
  checking: { ...BASE, phase: "checking", detected: null, chosen: null, downloadedBytes: 0 },
  choosing: { ...BASE, phase: "choosing", chosen: null, downloadedBytes: 0 },
  large: {
    ...BASE,
    phase: "choosing",
    detected: "large",
    chosen: null,
    ramGib: 64,
    downloadedBytes: 0,
    options: BASE.options.map((o) => ({ ...o, fits: true })),
  },
  small: {
    ...BASE,
    phase: "choosing",
    detected: "small",
    chosen: null,
    ramGib: 16,
    downloadedBytes: 0,
    options: BASE.options.map((o) => ({ ...o, fits: o.tier === "small" })),
  },
  unsupported: {
    ...BASE,
    phase: "choosing",
    detected: "unsupported",
    chosen: null,
    ramGib: 8,
    downloadedBytes: 0,
    options: BASE.options.map((o) => ({ ...o, fits: false })),
  },
  lowDisk: {
    ...BASE,
    phase: "choosing",
    detected: "unsupported",
    chosen: null,
    freeDiskBytes: 3e9,
    downloadedBytes: 0,
    options: BASE.options.map((o) => ({ ...o, fits: false })),
  },
  downloading: downloading({}),
  paused: downloading({ phase: "paused", paused: "person" }),
  offline: downloading({ phase: "paused", paused: "offline", error: "offline" }),
  battery: downloading({ phase: "paused", paused: "battery" }),
  diskFull: downloading({ phase: "paused", error: "diskFull", neededBytes: 6e9 }),
  installing: downloading({ phase: "installing", downloadedBytes: 8.4e9, etaSeconds: null }),
  failed: downloading({ phase: "failed", error: "engineFailed", etaSeconds: null }),
};

/** Maya's computer is set up. A visit with `?mockSetup=<name>` starts from one of the other states. */
export function setupSeed(): SetupView {
  const name =
    typeof location === "undefined" ? null : new URLSearchParams(location.search).get("mockSetup");
  const found = name ? SETUP_SCENARIOS[name] : undefined;
  return structuredClone(found ?? BASE);
}

export function advancedSeed(counts: { memories: number; items: number }): AdvancedView {
  return {
    tier: "standard",
    ramGib: 32,
    gpu: "Radeon 780M (iGPU)",
    npu: false,
    components: [
      { role: "mind", name: "Qwen3.5-9B-GGUF", bytes: 5.9e9, backend: "vulkan", loaded: true },
      { role: "speech", name: "Whisper-Small", bytes: 0.5e9, backend: "cpu", loaded: false },
      { role: "voice", name: "kokoro-v1", bytes: 0.3e9, backend: "cpu", loaded: false },
      {
        role: "recall",
        name: "Qwen3-Embedding-0.6B-GGUF",
        bytes: 0.6e9,
        backend: "cpu",
        loaded: true,
      },
    ],
    engineVersion: "8.2.2",
    enginePort: 8020,
    engineUptimeSeconds: 273_600,
    restarts: 0,
    storeVersion: 1,
    memoryCount: counts.memories,
    itemCount: counts.items,
    integrityOk: true,
    lastTidy: { from: "02:10", to: "02:16", items: 14, merged: 3, letGo: 3 },
    agents: false,
    logDir: "~/.local/share/nocturne/logs",
  };
}
