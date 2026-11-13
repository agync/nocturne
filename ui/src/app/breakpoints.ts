/** Below this width the rail collapses into Menu and the keep/ask bar docks at the bottom. */
export const NARROW_PX = 720;
/** Below this width the kept-today column drops under the page. */
export const MEDIUM_PX = 1024;

const BASE_FONT_PX = 16;

export type ShellSize = "narrow" | "medium" | "wide";

/**
 * Breakpoints scale with the text size, so a person who enlarges text gets the narrow layout
 * earlier instead of a squeezed wide one.
 */
export function shellSize(widthPx: number, rootFontPx: number): ShellSize {
  const scale = rootFontPx > 0 ? rootFontPx / BASE_FONT_PX : 1;
  if (widthPx < NARROW_PX * scale) return "narrow";
  if (widthPx < MEDIUM_PX * scale) return "medium";
  return "wide";
}
