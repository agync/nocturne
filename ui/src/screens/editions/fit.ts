import type { PageLine, PageView } from "../../engine/bindings";

export type PaperSize = "a4" | "letter" | "receipt";

/** The most lines each part of the sheet holds. Letter's page box is a little shorter. */
export const NEEDS_YOU_MAX = 6;
export const CHANGED_MAX: Record<PaperSize, number> = { a4: 6, letter: 5, receipt: 6 };
/** Let go is one line on paper and a short list on a receipt. */
export const LET_GO_NAMED: Record<PaperSize, number> = { a4: 3, letter: 3, receipt: 3 };

export interface Fitted {
  needsYou: PageLine[];
  /** Needs-you lines that did not fit, with the ones the page itself left out. */
  needsYouMore: number;
  changed: PageLine[];
  changedMore: number;
  /** What was let go, as many lines as fit. */
  letGo: PageLine[];
  letGoMore: number;
}

const live = (lines: PageLine[]) => lines.filter((l) => !l.forgotten);

/** What a let-go line is called on paper. */
export const letGoName = (line: PageLine): string => line.subject ?? line.text;

/** What fits on one sheet, in the order that matters: needs you, the question, what changed, let go. */
export function fit(page: PageView, size: PaperSize): Fitted {
  const needs = live(page.needsYou);
  const changed = live(page.changed);
  const letGo = live(page.letGo);
  const cap = CHANGED_MAX[size];
  const named = LET_GO_NAMED[size];
  return {
    needsYou: needs.slice(0, NEEDS_YOU_MAX),
    needsYouMore: Math.max(0, needs.length - NEEDS_YOU_MAX) + page.moreNeedsYou,
    changed: changed.slice(0, cap),
    changedMore: Math.max(0, changed.length - cap) + page.moreChanged,
    letGo: letGo.slice(0, named),
    letGoMore: Math.max(0, letGo.length - named),
  };
}
