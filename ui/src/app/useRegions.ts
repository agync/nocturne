import { type RefObject, useEffect } from "react";

const ORDER = ["keep", "main", "rail", "toast"] as const;

function focusRegion(region: HTMLElement): void {
  // The page region takes focus itself (it is tabindex -1); the others hand it to their first control.
  const target =
    region.dataset.region === "main" || region.matches("button, a, textarea, input")
      ? region
      : (region.querySelector<HTMLElement>(
          '[aria-current="page"], textarea, a[href], button:not([disabled])',
        ) ?? region);
  target.focus();
}

/** F6 moves focus to the next region (keep or ask, page, rail); Shift+F6 goes back. */
export function useRegionCycling(root: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "F6" || !root.current) return;
      // Toasts live outside the shell, so the whole document is searched for them.
      const regions = ORDER.map((name) =>
        (name === "toast" ? document : root.current)?.querySelector<HTMLElement>(
          `[data-region="${name}"]`,
        ),
      ).filter((el): el is HTMLElement => Boolean(el));
      if (regions.length === 0) return;
      event.preventDefault();
      const active = document.activeElement;
      const current = regions.findIndex((el) => el.contains(active));
      const step = event.shiftKey ? -1 : 1;
      const next = current === -1 ? 0 : (current + step + regions.length) % regions.length;
      focusRegion(regions[next] as HTMLElement);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [root]);
}
