import type { PageLine, PageView, SpokenKind } from "../bindings";

/** How many lines of "what changed" and "coming up" are read. */
const TOP = 6;
/** How many let-go items are named before "and N more". */
const NAMED_LET_GO = 2;

/** One part of the spoken page: read on its own, so the player can skip between parts. */
export interface ScriptSection {
  kind: SpokenKind;
  text: string;
}

const plain = (text: string) => text.replaceAll("**", "").trim();

const live = (lines: PageLine[]) => lines.filter((l) => !l.forgotten);

function joined(items: string[], last: string): string {
  if (items.length <= 1) return items[0] ?? "";
  return `${items.slice(0, -1).join(", ")} ${last} ${items[items.length - 1]}`;
}

function spokenLine(line: PageLine): string {
  const text = plain(line.text);
  const choices = line.choices.map((c) => plain(c.text)).filter((c) => c !== "");
  if (line.kind === "which" && choices.length >= 2) {
    const question =
      choices.length === 2
        ? `Was it ${choices[0]}, or ${choices[1]}?`
        : `Was it ${joined(choices, "or")}?`;
    return text === "" ? question : `${text} ${question}`;
  }
  return text;
}

function section(kind: SpokenKind, lines: string[]): ScriptSection[] {
  const text = lines.filter((l) => l.trim() !== "").join(" ");
  return text === "" ? [] : [{ kind, text }];
}

/**
 * What the spoken page says, in the real app's order: the opening, what needs the person, what
 * changed, what is coming up, what was let go, and the question. This mirrors the engine's own
 * script (`src-tauri/src/editions/script.rs`), for the audio edition of a page.
 */
export function audioScript(page: PageView): ScriptSection[] {
  const named = live(page.letGo)
    .map((l) => plain(l.subject ?? l.text))
    .filter((s) => s !== "");
  const shown = named.slice(0, NAMED_LET_GO);
  const more = named.length - shown.length;
  const names = more > 0 ? [...shown, `${more} more`] : shown;
  return [
    ...section("opening", [plain(page.headline), plain(page.dek)]),
    ...section("needsYou", live(page.needsYou).map(spokenLine)),
    ...section("changed", live(page.changed).slice(0, TOP).map(spokenLine)),
    ...section("comingUp", live(page.upcoming).slice(0, TOP).map(spokenLine)),
    ...(named.length > 0 ? section("letGo", [`Nocturne let go of ${joined(names, "and")}.`]) : []),
    ...(page.question && page.question.answered === null
      ? section("question", [plain(page.question.text), "You can answer it on today’s page."])
      : []),
  ];
}
