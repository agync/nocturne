import type { Platform } from "./types";

export async function createPlatform(): Promise<Platform> {
  if (import.meta.env.VITE_ENGINE === "mock") {
    const { WebPlatform } = await import("./web");
    return new WebPlatform();
  }
  if (import.meta.env.VITE_ENGINE === "http") {
    const { captureToken } = await import("../engine/httpAuth");
    const { HttpPlatform } = await import("./http");
    captureToken();
    return new HttpPlatform().start();
  }
  const { TauriPlatform } = await import("./tauri");
  return new TauriPlatform();
}

export type { Platform, SystemTheme, Unsubscribe } from "./types";
