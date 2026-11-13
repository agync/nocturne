import type { PageLine, PageView, QuestionChoice } from "../../engine/bindings";

/** What the person has just done, shown at once while the engine catches up. */
export interface Overrides {
  done: string[];
  /** line id -> the memory chosen. */
  chosen: Record<string, string>;
  restored: string[];
  /** Lines whose memory has just been forgotten for good. */
  forgotten: string[];
  answer: QuestionChoice | null;
}

export const NO_OVERRIDES: Overrides = {
  done: [],
  chosen: {},
  restored: [],
  forgotten: [],
  answer: null,
};

function patchLine(line: PageLine, o: Overrides): PageLine {
  if (o.forgotten.includes(line.id)) {
    return { ...line, forgotten: true, text: "", receipts: [], choices: [] };
  }
  if (o.done.includes(line.id) || o.restored.includes(line.id)) return { ...line, done: true };
  const chosen = o.chosen[line.id];
  if (chosen) return { ...line, done: true, memoryIds: [chosen] };
  return line;
}

export function applyOverrides(page: PageView, o: Overrides): PageView {
  return {
    ...page,
    needsYou: page.needsYou.map((l) => patchLine(l, o)),
    changed: page.changed.map((l) => patchLine(l, o)),
    letGo: page.letGo.map((l) => patchLine(l, o)),
    question: page.question && o.answer ? { ...page.question, answered: o.answer } : page.question,
  };
}
