// The app draws edge to edge, and the web view reports no safe area for the system bars. The
// native side puts the bars on the page as `--ph-inset-*` each time they change; this asks for
// them once at start, in case the page loaded after the first change.

import { invoke } from "@tauri-apps/api/core";

export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export function applyInsets(insets: Insets, root: HTMLElement = document.documentElement): void {
  for (const side of ["top", "right", "bottom", "left"] as const) {
    const px = Number.isFinite(insets[side]) ? Math.max(0, insets[side]) : 0;
    root.style.setProperty(`--ph-inset-${side}`, `${px}px`);
  }
}

export async function startInsets(root: HTMLElement = document.documentElement): Promise<void> {
  root.classList.add("ph-native");
  const insets = await invoke<Insets>("plugin:nocturne-system|insets").catch(() => null);
  if (insets && insets.top + insets.bottom > 0) applyInsets(insets, root);
}
