import { useEffect } from "react";
import type { Theme } from "../engine/bindings";
import { useSettings } from "../engine/context";
import { usePlatform } from "../platform/context";
import type { SystemTheme } from "../platform/types";

/**
 * `data-theme` comes from the setting when it is light or dark, else from the platform theme,
 * else it is left off so `prefers-color-scheme` decides.
 */
export function resolveTheme(setting: Theme, platform: SystemTheme | null): SystemTheme | null {
  if (setting === "light" || setting === "dark") return setting;
  return platform;
}

export function applyTheme(
  theme: SystemTheme | null,
  root: HTMLElement = document.documentElement,
) {
  if (theme) root.setAttribute("data-theme", theme);
  else root.removeAttribute("data-theme");
}

export function useThemeSync(): void {
  const platform = usePlatform();
  const { data: settings } = useSettings();
  const setting = settings?.theme ?? "system";

  useEffect(() => {
    let live = true;
    let system: SystemTheme | null = null;
    const apply = () => applyTheme(resolveTheme(setting, system));
    apply();
    void platform.currentTheme().then((theme) => {
      if (!live) return;
      system = theme;
      apply();
    });
    const off = platform.onThemeChanged((theme) => {
      system = theme;
      apply();
    });
    return () => {
      live = false;
      off();
    };
  }, [platform, setting]);
}
