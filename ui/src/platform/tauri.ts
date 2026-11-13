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
    const picked = await invoke<string | null>("plugin:dialog|open", {
      options: { multiple: false, directory: true },
    });
    return picked ?? null;
  }

  async openPath(path: string): Promise<void> {
    await invoke("plugin:opener|open_path", { path });
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

  async pickFiles(): Promise<string[]> {
    const picked = await invoke<string[] | null>("plugin:dialog|open", {
      options: { multiple: true, directory: false },
    });
    return picked ?? [];
  }
}
