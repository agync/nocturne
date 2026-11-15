import { useEffect } from "react";
import { applyTheme, syncedTheme } from "../../app/theme";

/**
 * Holds the window in the dark theme through every setup step, whatever the setting says, and puts
 * back the theme the sync last asked for afterwards. The sync may run again meanwhile, so it is
 * watched, and what it asks for then is what comes back.
 */
export function useNight(active: boolean): void {
  useEffect(() => {
    if (!active) return;
    const root = document.documentElement;
    const hold = () => {
      if (root.getAttribute("data-theme") !== "dark") root.setAttribute("data-theme", "dark");
    };
    hold();
    const watch = typeof MutationObserver === "function" ? new MutationObserver(hold) : null;
    watch?.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      watch?.disconnect();
      applyTheme(syncedTheme(), root);
    };
  }, [active]);
}
