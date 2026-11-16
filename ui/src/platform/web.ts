import type { Platform, SystemTheme, Unsubscribe } from "./types";

const DARK = "(prefers-color-scheme: dark)";

/** The browser: DOM drag events (paths are file names) and `matchMedia`. */
/** A hatched square, standing in for the person's own picture when there is no disk to read. */
const PLACEHOLDER =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="240" viewBox="0 0 320 240">' +
      '<rect width="320" height="240" fill="#FBFAF6"/>' +
      '<path d="M-40 280L280-40M-20 280L300-40M0 280L320-40M20 280L340-40M40 280L360-40M60 280L380-40M80 280L400-40M-60 280L260-40M-80 280L240-40" stroke="#DAD5CA" stroke-width="2"/>' +
      "</svg>",
  );

export class WebPlatform implements Platform {
  private readonly askListeners = new Set<(question: string) => void>();

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

  async pickFolder(): Promise<string | null> {
    return "Chosen folder";
  }

  async openPath(path: string): Promise<void> {
    // What the preview holds in memory opens in a new tab, where the browser plays or shows it.
    if (path.startsWith("blob:")) {
      window.open(path, "_blank", "noopener");
      return;
    }
    console.info(`Would open ${path}`);
  }

  async openOriginal(itemId: string, shown: string): Promise<void> {
    if (shown.startsWith("blob:")) return this.openPath(shown);
    console.info(`Would open the original of ${itemId}`);
  }

  async openLogs(): Promise<void> {
    console.info("Would open the logs");
  }

  async openDataFolder(): Promise<void> {
    console.info("Would open the data folder");
  }

  originalSrc(path: string): string {
    // The preview holds what was just kept in memory and points at it with a blob address.
    return path.startsWith("blob:") ? path : PLACEHOLDER;
  }

  inQuickCapture(): boolean {
    return false;
  }

  async closeQuickCapture(): Promise<void> {
    window.location.hash = "#/today";
  }

  async fitQuickCapture(): Promise<void> {}

  async handOffAsk(question: string): Promise<void> {
    for (const cb of [...this.askListeners]) cb(question);
  }

  onAskHandOff(cb: (question: string) => void): Unsubscribe {
    this.askListeners.add(cb);
    return () => {
      this.askListeners.delete(cb);
    };
  }

  async openInMain(path: string): Promise<void> {
    window.location.hash = `#${path}`;
  }

  onOpenInMain(_cb: (path: string) => void): Unsubscribe {
    // One window: openInMain moves it directly, so there is nothing to hear.
    return () => undefined;
  }

  async print(): Promise<void> {
    window.print();
  }

  pickFiles(): Promise<string[]> {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      input.accept = "image/*,application/pdf,audio/*,text/*";
      input.hidden = true;
      const finish = (paths: string[]) => {
        input.remove();
        resolve(paths);
      };
      input.addEventListener("change", () => finish([...(input.files ?? [])].map((f) => f.name)));
      input.addEventListener("cancel", () => finish([]));
      document.body.append(input);
      input.click();
    });
  }
}
