import type { AnsweredBy, AskPlace } from "../bindings";

/**
 * Where the mock answers a question. Online answers is never on in the mock yet, so every
 * answer is written on this computer; a question the person kept here says so.
 */
export function answeredBy(place: AskPlace | undefined): AnsweredBy {
  return { type: "computer", reason: place === "thisComputer" ? "chosen" : null };
}
