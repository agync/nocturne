import type { EngineClient } from "./client";

/** Chosen at build time: `VITE_ENGINE=mock` for tests, the browser and design review, else Tauri. */
export async function createEngine(): Promise<EngineClient> {
  if (import.meta.env.VITE_ENGINE === "mock") {
    const { createMockEngine } = await import("./mock/engine");
    return createMockEngine();
  }
  const { TauriEngine } = await import("./tauri");
  // The quick capture window may not listen to events: it has no need to, and what travels on
  // them (answers, kept items) is not for it.
  return new TauriEngine({ listen: !window.location.hash.startsWith("#/quick-capture") });
}

export type { EngineClient, EngineEventName, EngineEvents, Unsubscribe } from "./client";
