// The app draws edge to edge, and the web view reports no safe area for the system bars. The
// native side puts the bars on the page as `--ph-inset-*` each time they change, and the
// keyboard as `--ph-inset-keyboard`; this asks for them once at start, in case the page loaded
// after the first change.

import { invoke } from "@tauri-apps/api/core";

export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
  keyboard?: number;
}

const px = (n: number | undefined) => (Number.isFinite(n) ? Math.max(0, n as number) : 0);

export function applyInsets(insets: Insets, root: HTMLElement = document.documentElement): void {
  for (const side of ["top", "right", "bottom", "left", "keyboard"] as const) {
    root.style.setProperty(`--ph-inset-${side}`, `${px(insets[side])}px`);
  }
  root.classList.toggle("ph-keyboard", px(insets.keyboard) > 0);
}

/** The field being typed in stays above the keyboard, which the web view does not do itself. */
export function keepFieldInView(doc: Document = document): void {
  const field = doc.activeElement;
  if (!(field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement)) return;
  field.scrollIntoView({ block: "center", behavior: "smooth" });
}

export async function startInsets(root: HTMLElement = document.documentElement): Promise<void> {
  root.classList.add("ph-native");
  window.addEventListener("ph-insets", () => keepFieldInView());
  document.addEventListener("focusin", () => {
    if (root.classList.contains("ph-keyboard")) keepFieldInView();
  });
  const insets = await invoke<Insets>("plugin:nocturne-system|insets").catch(() => null);
  if (insets && insets.top + insets.bottom > 0) applyInsets(insets, root);
}
