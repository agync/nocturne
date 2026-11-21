import type { Platform } from "../platform/types";
import type { EngineClient } from "./client";

/** Chosen at build time: `VITE_ENGINE=mock` for tests, the browser and design review, `http` for the development preview
 * (`--mode preview-real`), else Tauri. */
export async function createEngine(
  window: Pick<Platform, "inQuickCapture">,
): Promise<EngineClient> {
  if (import.meta.env.VITE_ENGINE === "mock") {
    const { createMockEngine } = await import("./mock/engine");
    const engine = createMockEngine();
    // `?mockPhones=1` pairs two phones that bring a calendar and health, for design review.
    if (typeof location !== "undefined" && new URLSearchParams(location.search).has("mockPhones")) {
      const { withPhones } = await import("./mock/phones");
      return withPhones(engine, import.meta.env.VITE_MOCK_NOW || "2026-11-13T07:02:00Z");
    }
    return engine;
  }
  if (import.meta.env.VITE_ENGINE === "http") {
    // The development preview: the real engine behind a local server. Never part of a package.
    const { HttpEngine } = await import("./http");
    const { captureToken, currentToken } = await import("./httpAuth");
    captureToken();
    return new HttpEngine({ token: currentToken });
  }
  const { TauriEngine } = await import("./tauri");
  // The quick capture window may not listen to events: it has no need to, and what travels on
  // them (answers, kept items) is not for it. The window's own label decides, not the route, so
  // a main window that reloads on any route still listens.
  return new TauriEngine({ listen: !window.inQuickCapture() });
}

export type { EngineClient, EngineEventName, EngineEvents, Unsubscribe } from "./client";
