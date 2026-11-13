import type { EngineClient } from "./client";

/** Chosen at build time: `VITE_ENGINE=mock` for tests, the browser and design review, else Tauri. */
export async function createEngine(): Promise<EngineClient> {
  if (import.meta.env.VITE_ENGINE === "mock") {
    const { createMockEngine } = await import("./mock/engine");
    return createMockEngine();
  }
  const { TauriEngine } = await import("./tauri");
  return new TauriEngine();
}

export type { EngineClient, EngineEventName, EngineEvents, Unsubscribe } from "./client";
