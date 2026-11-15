import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import type { Platform, SystemTheme, Unsubscribe } from "./types";

/** Defers to a promise-returning subscription while handing back a synchronous unsubscribe. */
function lazyUnlisten(pending: Promise<() => void>): Unsubscribe {
  let cancelled = false;
  // A window without permission to listen (quick capture) is refused: that is not an error.
  void pending
    .then((unlisten) => {
      if (cancelled) unlisten();
    })
    .catch(() => undefined);
  return () => {
    cancelled = true;
    void pending.then((unlisten) => unlisten()).catch(() => undefined);
  };
}

/** Asks the app for files dropped on the quick capture window until unsubscribed. */
function pollQuickDrop(onDrop: (paths: string[]) => void): Unsubscribe {
  let stopped = false;
  const timer = setInterval(() => {
    invoke<string[]>("take_quick_drop")
      .then((paths) => {
        if (!stopped && paths.length > 0) onDrop(paths);
      })
      .catch(() => undefined);
  }, QUICK_DROP_POLL_MS);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}

/** The event the quick-capture window sends to the main window with a question. */
const ASK_HANDOFF = "ask-handoff";
const OPEN_IN_MAIN = "open-in-main";
/** How often the quick capture window asks the app what was dropped on it. */
const QUICK_DROP_POLL_MS = 400;

export class TauriPlatform implements Platform {
  onFileDrop(
    onDrop: (paths: string[]) => void,
    onHover: (count: number | null) => void,
  ): Unsubscribe {
    // The quick capture window may not listen to events, so the drop is not delivered to it:
    // the app holds the paths and this window asks for them.
    if (this.inQuickCapture()) return pollQuickDrop(onDrop);
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

  async openDataFolder(): Promise<void> {
    await invoke("open_data_folder");
  }

  originalSrc(path: string): string {
    return convertFileSrc(path);
  }

  inQuickCapture(): boolean {
    return getCurrentWindow().label === "quick-capture";
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

  async print(): Promise<void> {
    await invoke("print_page");
  }

  async pickFiles(): Promise<string[]> {
    return (await invoke<string[] | null>("pick_files")) ?? [];
  }
}
