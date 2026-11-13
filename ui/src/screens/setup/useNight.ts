import { useEffect } from "react";

/**
 * Holds the window in the dark theme through every setup step, whatever the setting says, and puts
 * the old theme back afterwards. The theme sync may run again meanwhile, so it is watched.
 */
export function useNight(active: boolean): void {
  useEffect(() => {
    if (!active) return;
    const root = document.documentElement;
    const before = root.getAttribute("data-theme");
    const hold = () => {
      if (root.getAttribute("data-theme") !== "dark") root.setAttribute("data-theme", "dark");
    };
    hold();
    const watch = typeof MutationObserver === "function" ? new MutationObserver(hold) : null;
    watch?.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      watch?.disconnect();
      if (before === null) root.removeAttribute("data-theme");
      else root.setAttribute("data-theme", before);
    };
  }, [active]);
}
