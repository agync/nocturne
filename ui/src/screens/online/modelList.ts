// The service's list of names, as the phone's step 2 and the computer's Connect show it: in order,
// searched, and what the name typed says against it. Nothing here guesses what a name is for.

import type { CheckOutcome, ModelList } from "../../engine/bindings";

/** A to Z ignoring capitals, numbers in number order. */
const order = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

/** The names as the list shows them: in order, each once. */
export function sortNames(names: readonly string[]): string[] {
  return [...new Set(names)].sort((a, b) => order.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0));
}

/** More names than this, and the list opens with its search. */
export const SEARCH_ABOVE = 8;

/** The names with `query` in them, ignoring capitals. */
export function matching(names: readonly string[], query: string): string[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...names];
  return names.filter((n) => n.toLowerCase().includes(q));
}

/** `name` cut where `query` matches, ignoring capitals, so the matching part can be marked. */
export function marked(name: string, query: string): { text: string; match: boolean }[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [{ text: name, match: false }];
  const parts: { text: string; match: boolean }[] = [];
  const lower = name.toLowerCase();
  let from = 0;
  for (let at = lower.indexOf(q); at >= 0; at = lower.indexOf(q, from)) {
    if (at > from) parts.push({ text: name.slice(from, at), match: false });
    parts.push({ text: name.slice(at, at + q.length), match: true });
    from = at + q.length;
  }
  if (from < name.length) parts.push({ text: name.slice(from), match: false });
  return parts;
}

/** What the name in the field says against the list, when one is known. */
export type NameAgainstList =
  | { kind: "none" }
  | { kind: "on" }
  | { kind: "notOn" }
  /** Not on it, but one listed name differs only in capitals. */
  | { kind: "capitals"; listed: string };

export function against(name: string, names: readonly string[] | null): NameAgainstList {
  const typed = name.trim();
  if (!names || names.length === 0 || typed === "") return { kind: "none" };
  if (names.includes(typed)) return { kind: "on" };
  const listed = names.find((n) => n.toLowerCase() === typed.toLowerCase());
  return listed ? { kind: "capitals", listed } : { kind: "notOn" };
}

/** Why a look failed, as the screens tell it apart: typing a name can still help, or not. */
export type ListProblem = Exclude<CheckOutcome["type"], "works" | "cancelled">;

/** A failed look where typing the name can still work: the service was busy or failed itself. */
export function typingHelps(problem: ListProblem): boolean {
  return problem === "busy" || problem === "serverError";
}

/** The list control and the line under the field, state by state. */
export type ListState =
  | { name: "idle" }
  | { name: "looking" }
  | { name: "listed"; names: string[] }
  | { name: "none" }
  | { name: "failed"; problem: ListProblem };

/** The state a look's answer leaves. */
export function afterLook(list: ModelList): ListState {
  if (list.type === "listed") return { name: "listed", names: sortNames(list.names) };
  if (list.type === "none") return { name: "none" };
  const problem = list.problem.type;
  if (problem === "works" || problem === "cancelled") return { name: "none" };
  return { name: "failed", problem };
}
