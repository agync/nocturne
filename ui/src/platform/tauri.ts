import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
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

  async pickFiles(): Promise<string[]> {
    const picked = await invoke<string[] | null>("plugin:dialog|open", {
      options: { multiple: true, directory: false },
    });
    return picked ?? [];
  }
}
