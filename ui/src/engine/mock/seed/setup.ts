import type { AdvancedView, SetupView } from "../../bindings";

/** Getting ready, and the advanced details. The Setup and Settings screens own this file. */
export function setupSeed(): SetupView {
  return {
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
}

export function advancedSeed(counts: { memories: number; items: number }): AdvancedView {
  return {
    tier: "standard",
    ramGib: 32,
    gpu: "Integrated graphics",
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
    engineVersion: "mock",
    enginePort: null,
    engineUptimeSeconds: 0,
    restarts: 0,
    storeVersion: 1,
    memoryCount: counts.memories,
    itemCount: counts.items,
    integrityOk: true,
    lastTidy: { from: "02:10", to: "02:16", items: 14, merged: 3, letGo: 3 },
    agents: false,
    logDir: "logs",
  };
}
