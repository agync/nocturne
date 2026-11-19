import { addDays, localDay } from "../../copy/format";
import type { ItemView, Question } from "../bindings";

/** `?mockQuestion=journal` makes today's page ask a question from the journal. */
export function journalQuestionFromAddress(): boolean {
  if (typeof location === "undefined") return false;
  return new URLSearchParams(location.search).get("mockQuestion") === "journal";
}

const SENTENCE = "Parents’ evening at six. Not looking forward to it.";

/** An entry block kept on the event's day, and the question about the sentence in it. */
export function journalQuestionSeed(now: Date): { item: ItemView; question: Question } {
  const eventDay = addDays(localDay(now), -1);
  const [y, m, d] = eventDay.split("-").map(Number) as [number, number, number];
  const keptAt = new Date(y, m - 1, d, 8, 12).toISOString();
  const item: ItemView = {
    id: "j-block-question",
    kind: "text",
    title: SENTENCE,
    source: "journal",
    trust: "trusted",
    keptAt,
    status: "understood",
    reason: null,
    durationMs: null,
    pages: null,
    fileName: null,
    hasOriginal: false,
    originalPath: null,
    private: false,
    memoryCount: 1,
    understoodInMs: null,
    summary: null,
    text: `${SENTENCE} Leo has been quiet about school all week.`,
    language: null,
    fromDevice: null,
    journal: "entry",
  };
  return {
    item,
    question: {
      kind: "journal",
      text: SENTENCE,
      memoryId: null,
      receipt: {
        itemId: item.id,
        kind: "text",
        source: "journal",
        label: item.title,
        keptAt,
        durationMs: null,
        span: null,
        unconfirmed: false,
        fromDevice: null,
      },
      lastAsked: null,
      answered: null,
      journal: {
        itemId: item.id,
        voice: false,
        writtenOn: eventDay,
        eventDay,
        answerItemId: null,
        line: null,
      },
    },
  };
}
