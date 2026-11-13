import type { Platform, SystemTheme, Unsubscribe } from "./types";

const DARK = "(prefers-color-scheme: dark)";

/** The browser: DOM drag events (paths are file names) and `matchMedia`. */
export class WebPlatform implements Platform {
  onFileDrop(
    onDrop: (paths: string[]) => void,
    onHover: (count: number | null) => void,
  ): Unsubscribe {
    let depth = 0;
    const hasFiles = (event: DragEvent) => event.dataTransfer?.types.includes("Files") ?? false;

    const enter = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      depth += 1;
      if (depth === 1) onHover(Math.max(1, event.dataTransfer?.items.length ?? 1));
    };
    const over = (event: DragEvent) => {
      if (hasFiles(event)) event.preventDefault();
    };
    const leave = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) onHover(null);
    };
    const drop = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      depth = 0;
      onHover(null);
      const names = [...(event.dataTransfer?.files ?? [])].map((f) => f.name);
      if (names.length > 0) onDrop(names);
    };

    window.addEventListener("dragenter", enter);
    window.addEventListener("dragover", over);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragover", over);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("drop", drop);
    };
  }

  onThemeChanged(cb: (theme: SystemTheme) => void): Unsubscribe {
    if (typeof matchMedia !== "function") return () => undefined;
    const query = matchMedia(DARK);
    const listener = (event: MediaQueryListEvent) => cb(event.matches ? "dark" : "light");
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }

  async currentTheme(): Promise<SystemTheme | null> {
    if (typeof matchMedia !== "function") return null;
    return matchMedia(DARK).matches ? "dark" : "light";
  }

  pickFiles(): Promise<string[]> {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      input.addEventListener("change", () => resolve([...(input.files ?? [])].map((f) => f.name)));
      input.addEventListener("cancel", () => resolve([]));
      input.click();
    });
  }
}
