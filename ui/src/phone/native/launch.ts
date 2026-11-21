// What the app was opened for from outside: a long press on its icon ("Keep a note", "Record a
// voice note", "Check in"), or another app's share menu. The phone's side holds each until the
// screens take it, once.

import { invoke } from "@tauri-apps/api/core";

/** A shortcut on the app's icon. */
export type LaunchRoute = "keep" | "record" | "checkin";

/** A file another app shared, copied into the app's own space until it is kept. */
export interface SharedFile {
  /** Its name in the app's space, which fetches it. */
  name: string;
  /** Its own name, as the other app gave it. */
  shown: string;
  mime: string;
  size: number;
}

/** What another app shared: text, a link, or files. */
export interface Shared {
  text: string | null;
  subject: string | null;
  files: SharedFile[];
}

const ROUTES: readonly LaunchRoute[] = ["keep", "record", "checkin"];

export async function takeLaunch(): Promise<LaunchRoute | null> {
  const route = await invoke<string | null>("plugin:nocturne-system|take_launch").catch(() => null);
  return ROUTES.find((r) => r === route) ?? null;
}

export async function takeShared(): Promise<Shared | null> {
  const shared = await invoke<Shared | null>("plugin:nocturne-system|take_shared").catch(
    () => null,
  );
  if (!shared) return null;
  const text = shared.text?.trim() || null;
  return text || shared.files.length > 0 ? { ...shared, text } : null;
}

/** One shared file's bytes. */
export async function sharedFile(file: SharedFile): Promise<Blob> {
  const bytes = await invoke<ArrayBuffer | number[]>("plugin:nocturne-system|shared_file", {
    name: file.name,
  });
  const body = bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes);
  return new Blob([body], { type: file.mime || "application/octet-stream" });
}

/** The shared files are kept or let go: their copies leave the app's space. */
export function sharedDone(): Promise<void> {
  return invoke<void>("plugin:nocturne-system|shared_done").catch(() => undefined);
}

/** At start and each time the app comes to the front: what it was opened for, once. */
export function watchLaunch(
  onRoute: (route: LaunchRoute) => void,
  onShared: (shared: Shared) => void,
): () => void {
  const check = () => {
    void takeLaunch().then((r) => r && onRoute(r));
    void takeShared().then((s) => s && onShared(s));
  };
  const seen = () => {
    if (document.visibilityState === "visible") check();
  };
  check();
  document.addEventListener("visibilitychange", seen);
  return () => document.removeEventListener("visibilitychange", seen);
}
