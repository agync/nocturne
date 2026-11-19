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
  /** The line just added to a journal question's answer. */
  line: string | null;
}

export const NO_OVERRIDES: Overrides = {
  done: [],
  chosen: {},
  restored: [],
  forgotten: [],
  answer: null,
  line: null,
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

function patchQuestion(question: PageView["question"], o: Overrides): PageView["question"] {
  if (!question) return question;
  const answered = o.answer ?? question.answered;
  if (!question.journal || o.line === null) return o.answer ? { ...question, answered } : question;
  return { ...question, answered, journal: { ...question.journal, line: o.line } };
}

export function applyOverrides(page: PageView, o: Overrides): PageView {
  return {
    ...page,
    needsYou: page.needsYou.map((l) => patchLine(l, o)),
    changed: page.changed.map((l) => patchLine(l, o)),
    letGo: page.letGo.map((l) => patchLine(l, o)),
    question: patchQuestion(page.question, o),
  };
}
