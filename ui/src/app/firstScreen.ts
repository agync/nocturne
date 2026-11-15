import type { EngineState, SetupView } from "../engine/bindings";

/** Phases in which a chosen size is still on its way. */
const UNDER_WAY: ReadonlySet<SetupView["phase"]> = new Set([
  "choosing",
  "downloading",
  "paused",
  "installing",
]);

/**
 * Where the window opens: setup until a first size is in place, else today's page. Before the
 * engine answers, setup's view says what its record on disk does: no size asked for, or one that
 * never finished, which reads as paused. A size under way with nothing yet to read with is a first
 * setup; with something to read with, it is a change from an earlier size, and today's page opens.
 */
export function firstScreen(view: SetupView, state: EngineState | null): "/setup" | "/today" {
  if (view.phase === "done") return "/today";
  if (view.chosen === null) return "/setup";
  return UNDER_WAY.has(view.phase) && state === "gettingReady" ? "/setup" : "/today";
}
