import type { AskPlace } from "../../engine/bindings";

let next: AskPlace = "auto";

/**
 * The person's choice for the one question about to be asked from the bar. The bar sets it just
 * before it asks; the Ask page takes it when the question starts, which also resets it, so a
 * choice never carries over to the next question.
 */
export const askPlace = {
  set(place: AskPlace): void {
    next = place;
  },
  take(): AskPlace {
    const taken = next;
    next = "auto";
    return taken;
  },
};
