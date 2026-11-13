import { currentToken } from "../engine/httpAuth";
import { rememberFile } from "../engine/previewFiles";
import type { Unsubscribe } from "./types";
import { WebPlatform } from "./web";

/** How often the ticket that lets media elements fetch originals is renewed. */
const RENEW_MS = 6 * 3600 * 1000;

/**
 * The browser in front of the real engine: originals are shown from the preview server by
 * link, files are chosen in the page and travel as bytes, and everything that needs the desktop
 * (a folder dialog, the file manager, the logs) does nothing.
 */
export class HttpPlatform extends WebPlatform {
  private ticket: string | null = null;

  /** Fetch the ticket and keep it fresh. Media elements cannot send a header, so it rides in the link. */
  async start(fetcher: typeof fetch = (...a) => globalThis.fetch(...a)): Promise<this> {
    const renew = async () => {
      const token = currentToken();
      if (!token) return;
      try {
        const response = await fetcher("/api/ticket", {
          method: "POST",
          headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
          body: JSON.stringify({ kind: "media" }),
        });
        if (response.ok) this.ticket = ((await response.json()) as { ticket: string }).ticket;
      } catch {
        // Originals fail to show until the next try; nothing else is lost.
      }
    };
    await renew();
    setInterval(renew, RENEW_MS);
    return this;
  }

  override originalSrc(path: string): string {
    if (!path.startsWith("/originals/") && !path.startsWith("/audio/")) return path;
    return this.ticket ? `${path}?t=${this.ticket}` : path;
  }

  override async pickFolder(): Promise<string | null> {
    return null;
  }

  override async openPath(): Promise<void> {
    console.info("Opening folders is not available in a browser");
  }

  override async openOriginal(): Promise<void> {
    console.info("Opening a file is not available in a browser");
  }

  override async openLogs(): Promise<void> {
    console.info("Opening logs is not available in a browser");
  }

  override pickFiles(): Promise<string[]> {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      input.addEventListener("change", () => resolve([...(input.files ?? [])].map(rememberFile)));
      input.addEventListener("cancel", () => resolve([]));
      input.click();
    });
  }

  override onFileDrop(
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
      const keys = [...(event.dataTransfer?.files ?? [])].map(rememberFile);
      if (keys.length > 0) onDrop(keys);
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
}
