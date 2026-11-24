// The step's bar (`ph-foot`) on a phone: pinned to the bottom of the screen however long the words,
// unless it would take more than a third of the screen or the keyboard is up, and fading the last
// words into it while more continue below. The bar stays after the words in the page, so a screen
// reader meets the words first, and the page keeps room for it so nothing focused hides behind it.

/** What the bar does at one moment. */
export interface FootPlace {
  pinned: boolean;
  /** Words continue below the bar. */
  more: boolean;
}

export interface FootMeasure {
  /** The bar's own height. */
  foot: number;
  /** The screen's height. */
  viewport: number;
  keyboard: boolean;
  /** How far the page is scrolled, and how tall it is. */
  scrolled: number;
  height: number;
}

/** The bar pins while it takes no more than this share of the screen. */
export const MOST_OF_SCREEN = 1 / 3;

export function footPlace(m: FootMeasure): FootPlace {
  const pinned = !m.keyboard && m.foot <= m.viewport * MOST_OF_SCREEN;
  const more = pinned && m.scrolled + m.viewport < m.height - 1;
  return { pinned, more };
}

/**
 * Keeps every bar inside `root` placed as {@link footPlace} says, as the page scrolls, resizes,
 * changes or the keyboard comes and goes. Returns the way to stop.
 */
export function followFeet(root: HTMLElement): () => void {
  const doc = root.ownerDocument;
  const win = doc.defaultView;
  if (!win) return () => undefined;
  const place = () => {
    const html = doc.documentElement;
    let room = 0;
    for (const foot of root.querySelectorAll<HTMLElement>(".ph-foot")) {
      const p = footPlace({
        foot: foot.offsetHeight,
        viewport: win.innerHeight,
        keyboard: html.classList.contains("ph-keyboard"),
        scrolled: win.scrollY,
        height: html.scrollHeight,
      });
      foot.dataset.pinned = String(p.pinned);
      foot.dataset.more = String(p.more);
      if (p.pinned) room = Math.max(room, foot.offsetHeight);
    }
    html.style.setProperty("--ph-foot-room", `${room}px`);
  };
  place();
  win.addEventListener("scroll", place, { passive: true });
  win.addEventListener("resize", place);
  const watch = typeof ResizeObserver === "function" ? new ResizeObserver(place) : null;
  watch?.observe(root);
  const changes = new MutationObserver(place);
  changes.observe(root, { childList: true, subtree: true, characterData: true });
  changes.observe(doc.documentElement, { attributes: true, attributeFilter: ["class"] });
  return () => {
    win.removeEventListener("scroll", place);
    win.removeEventListener("resize", place);
    watch?.disconnect();
    changes.disconnect();
    doc.documentElement.style.removeProperty("--ph-foot-room");
  };
}
