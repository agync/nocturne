import type { PageLine } from "../../bindings";
import { line, type SeedContext } from "./common";

/** The things waiting on the person today: a form due, and a which-is-right. The Needs you screen owns this file. */
export function needsYouLines(ctx: SeedContext): PageLine[] {
  const { receipt, tuesdayText, thursdayText } = ctx;
  return [
    line(
      "line-trip",
      "due",
      "The Year 4 trip form is due **next Friday**. It needs a signature and £12.",
      ["mem-trip"],
      [receipt("item-letter")],
    ),
    line(
      "line-dentist",
      "which",
      "Your dentist appointment changed. Which is right?",
      ["mem-dentist-tue", "mem-dentist-thu"],
      [],
      {
        conflictId: "conflict-dentist",
        subject: "Dentist appointment",
        choices: [
          { memoryId: "mem-dentist-tue", text: tuesdayText, receipt: receipt("item-card") },
          {
            memoryId: "mem-dentist-thu",
            text: thursdayText,
            receipt: receipt("item-dentist-voice", { type: "time", startMs: 0, endMs: 8000 }),
          },
        ],
      },
    ),
  ];
}
