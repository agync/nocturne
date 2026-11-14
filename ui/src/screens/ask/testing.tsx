import { vi } from "vitest";
import type { AskDelta, AskPlace, OnlineView } from "../../engine/bindings";
import { createMockEngine, MockEngine } from "../../engine/mock/engine";

/**
 * A mock computer on which Online answers is in the given state (off when `null`). The view is
 * read the way the screens read it, through `getOnline`.
 */
export function onlineEngine(view: OnlineView | null): MockEngine {
  const engine = createMockEngine();
  if (view) vi.spyOn(engine.online, "view").mockReturnValue(view);
  return engine;
}

export type Step = Partial<AskDelta> & Pick<AskDelta, "phase">;

/** A mock computer that answers each question with the steps a test gives, and notes the place. */
export class ScriptedAnswers extends MockEngine {
  steps: Step[] = [];
  asked: Array<{ question: string; place: AskPlace | undefined }> = [];

  override async ask(question: string, place?: AskPlace): Promise<string> {
    this.asked.push({ question, place });
    const askId = `scripted-${this.asked.length}`;
    // Every step is delivered before the id is returned, so nothing depends on the clock. The
    // screen keeps what arrives early and replays it once it knows the id.
    for (const step of this.steps) {
      const delta: AskDelta = {
        askId,
        textDelta: null,
        receipts: [],
        memoryIds: [],
        near: [],
        found: null,
        etaSeconds: null,
        plain: false,
        ...step,
      };
      (this as unknown as { emit(name: string, delta: AskDelta): void }).emit("ask-delta", delta);
    }
    return askId;
  }
}

/** The steps of a one-memory answer, ending on the given terminal step. */
export async function oneMemoryAnswer(
  engine: MockEngine,
  last: Step = { phase: "done" },
): Promise<Step[]> {
  const all = await engine.listMemories({
    entityId: null,
    itemId: null,
    status: null,
    query: null,
    limit: null,
  });
  const swim = all.find((m) => m.id === "mem-swim");
  if (!swim) throw new Error("seed");
  return [
    { phase: "searching" },
    { phase: "reading", found: 1, receipts: swim.receipts, memoryIds: [swim.id] },
    { phase: "streaming", textDelta: "Leo swims **on Mondays**." },
    { ...last, memoryIds: [swim.id] },
  ];
}
