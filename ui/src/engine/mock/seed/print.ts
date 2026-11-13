import type { PageView } from "../../bindings";
import { line, type SeedContext } from "./common";

/** A day nobody lives: the one the print check asks for. */
export const PRINT_STRESS_DATE = "2026-01-01";

/**
 * The heaviest page a sheet has to hold: more needs-you and changed lines than fit, a line that
 * asks which of two things is right, long sentences, a question and let-go items. It exists so
 * the one-sheet check has a worst case to print.
 */
export function printStressPage(ctx: SeedContext): PageView {
  const { receipt, today } = ctx;
  const sentence = (n: number) =>
    `Item ${n}: confirm the booking for the Year 4 trip with **Mrs Okafor** before Friday, and bring the signed form and twelve pounds in an envelope.`;
  const needsYou = Array.from({ length: 8 }, (_, i) =>
    line(`stress-need-${i}`, "due", sentence(i + 1), [], [receipt("item-letter")]),
  );
  needsYou[1] = line(
    "stress-which",
    "which",
    "The dentist moved.",
    ["mem-dentist-1", "mem-dentist-2"],
    [],
    {
      subject: "Dentist appointment",
      choices: [
        {
          memoryId: "mem-dentist-1",
          text: "**Tue 18 Nov**, 14:00",
          receipt: receipt("item-letter"),
        },
        { memoryId: "mem-dentist-2", text: "**Thu 20 Nov**, 10:00", receipt: receipt("item-gran") },
      ],
    },
  );
  const changed = Array.from({ length: 9 }, (_, i) =>
    line(
      `stress-change-${i}`,
      "changed",
      `Change ${i + 1}: Leo’s swimming moved from Wednesday to **Monday**, after the pool changed its timetable again.`,
      [],
      [receipt("item-swim")],
    ),
  );
  const letGo = [
    "The old gym door code",
    "The pizza place on Elm Street",
    "Two bin-day notes",
    "A coupon",
  ].map((text, i) =>
    line(`stress-go-${i}`, "letGo", text, [], [], { subject: text, lastUsed: today }),
  );
  return {
    date: PRINT_STRESS_DATE,
    pageNumber: 99,
    edition: "print",
    variant: "heavy",
    headline: "Twelve things need you today.",
    dek: "Overnight Nocturne read 62 things you kept, noticed 19 changes and let 4 go.",
    needsYou,
    changed,
    letGo,
    upcoming: [],
    learned: [],
    question: {
      kind: "yesNo",
      text: "Is Ava still allergic to penicillin?",
      memoryId: null,
      receipt: null,
      lastAsked: null,
      answered: null,
    },
    moreNeedsYou: 4,
    moreChanged: 10,
    keptCount: 62,
    changesCount: 19,
    letGoCount: 4,
    catchUpDays: 0,
    privateSkipped: 2,
    pending: 0,
    tidiedFrom: null,
    tidiedTo: null,
    update: null,
  };
}
