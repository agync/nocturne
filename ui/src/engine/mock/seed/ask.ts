import type { MemoryView, Question } from "../../bindings";

/** Example questions the Ask screen can offer, and the one the page asks each morning. */
export const ASK_EXAMPLES: readonly string[] = [
  "When is the dentist?",
  "Is Leo allergic to nuts?",
  "When does my passport run out?",
];

/** Never about a memory kept forever: those are not re-asked. */
export function dailyQuestion(memories: MemoryView[]): Question {
  const memory = memories.find((m) => m.id === "mem-coach");
  if (!memory) throw new Error("Seed memory missing: mem-coach");
  return {
    kind: "yesNo",
    text: "Is Leo still doing swimming with Coach Sam?",
    memoryId: memory.id,
    receipt: memory.receipts[0] ?? null,
    lastAsked: null,
    answered: null,
  };
}
