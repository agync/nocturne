import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import type { Platform, SystemTheme, Unsubscribe } from "./types";

/** Defers to a promise-returning subscription while handing back a synchronous unsubscribe. */
function lazyUnlisten(pending: Promise<() => void>): Unsubscribe {
  let cancelled = false;
  void pending.then((unlisten) => {
    if (cancelled) unlisten();
  });
  return () => {
    cancelled = true;
    void pending.then((unlisten) => unlisten()).catch(() => undefined);
  };
}

/** The event the quick-capture window sends to the main window with a question. */
const ASK_HANDOFF = "ask-handoff";
const OPEN_IN_MAIN = "open-in-main";

export class TauriPlatform implements Platform {
  onFileDrop(
    onDrop: (paths: string[]) => void,
    onHover: (count: number | null) => void,
  ): Unsubscribe {
    return lazyUnlisten(
      getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter") onHover(payload.paths.length);
        else if (payload.type === "leave") onHover(null);
        else if (payload.type === "drop") {
          onHover(null);
          if (payload.paths.length > 0) onDrop(payload.paths);
        }
      }),
    );
  }

  onThemeChanged(cb: (theme: SystemTheme) => void): Unsubscribe {
    return lazyUnlisten(getCurrentWindow().onThemeChanged((event) => cb(event.payload)));
  }

  async currentTheme(): Promise<SystemTheme | null> {
    return (await getCurrentWindow().theme()) ?? null;
  }

  async pickFolder(): Promise<string | null> {
    return (await invoke<string | null>("pick_folder")) ?? null;
  }

  async openPath(path: string): Promise<void> {
    await invoke("open_export", { path });
  }

  async openOriginal(itemId: string): Promise<void> {
    await invoke("open_original", { itemId });
  }

  async openLogs(): Promise<void> {
    await invoke("open_logs");
  }

  originalSrc(path: string): string {
    return convertFileSrc(path);
  }

  async closeQuickCapture(): Promise<void> {
    await getCurrentWindow().close();
  }

  async handOffAsk(question: string): Promise<void> {
    await emitTo("main", ASK_HANDOFF, question);
    const main = await Window.getByLabel("main");
    await main?.show();
    await main?.setFocus();
  }

  onAskHandOff(cb: (question: string) => void): Unsubscribe {
    return lazyUnlisten(listen<string>(ASK_HANDOFF, (event) => cb(event.payload)));
  }

  async openInMain(path: string): Promise<void> {
    await emitTo("main", OPEN_IN_MAIN, path);
    const main = await Window.getByLabel("main");
    await main?.show();
    await main?.setFocus();
  }

  onOpenInMain(cb: (path: string) => void): Unsubscribe {
    return lazyUnlisten(listen<string>(OPEN_IN_MAIN, (event) => cb(event.payload)));
  }

  async pickFiles(): Promise<string[]> {
    return (await invoke<string[] | null>("pick_files")) ?? [];
  }
}
