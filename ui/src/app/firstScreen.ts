import type { SetupView } from "../engine/bindings";

/** Where the window opens: setup until a size has been chosen, else today's page. */
export function firstScreen(view: SetupView): "/setup" | "/today" {
  return view.phase !== "done" && view.chosen === null ? "/setup" : "/today";
}
