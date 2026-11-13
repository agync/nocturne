import { addDays, formatDay } from "../../../copy/format";
import type { MemoryView, PageView } from "../../bindings";
import { dailyQuestion } from "./ask";
import { line, type SeedContext } from "./common";
import { needsYouLines } from "./needs-you";

/** The morning pages, today's and the variants. The Today and Past pages screens own this file. */
export function pagesSeed(ctx: SeedContext, memories: MemoryView[]): PageView[] {
  const { today, at, receipt, friday, thursday, nextMonday } = ctx;
  const formatDayLabel = formatDay;
  const question = dailyQuestion(memories);

  const todayPage: PageView = {
    date: today,
    pageNumber: 23,
    edition: "screen",
    variant: "normal",
    headline: "Two things need you today.",
    dek: "Overnight Nocturne read 14 things you kept, noticed 3 changes and let 3 go.",
    needsYou: needsYouLines(ctx),
    changed: [
      line(
        "line-swim",
        "changed",
        "Leo’s swimming moved from Wednesday to **Monday**, same time.",
        ["mem-swim"],
        [receipt("item-swim")],
      ),
      line(
        "line-lake",
        "changed",
        "You decided to book the **Lake District** cottage for half term.",
        ["mem-lake"],
        [receipt("item-lake", { type: "time", startMs: 0, endMs: 42000 })],
      ),
      line(
        "line-gran",
        "changed",
        "Gran’s new number ends **4471**.",
        ["mem-gran"],
        [receipt("item-gran")],
      ),
    ],
    letGo: [
      line("line-gym", "letGo", "The old gym door code", ["mem-gym"], [receipt("item-gym")], {
        lastUsed: addDays(today, -90),
      }),
      line(
        "line-pizza",
        "letGo",
        "The pizza place on Elm Street",
        ["mem-pizza"],
        [receipt("item-pizza")],
        {
          lastUsed: addDays(today, -120),
        },
      ),
      line(
        "line-bin",
        "merged",
        "Two notes about bin day were merged into one",
        [],
        [receipt("item-bin")],
      ),
    ],
    upcoming: [],
    learned: [],
    question,
    moreNeedsYou: 0,
    moreChanged: 0,
    keptCount: 14,
    changesCount: 3,
    letGoCount: 3,
    catchUpDays: 0,
    privateSkipped: 0,
    pending: 1,
    tidiedFrom: at(0, 2, 10),
    tidiedTo: at(0, 2, 16),
    update: null,
  };

  const earlier = (offset: number, number: number, headline: string, dek: string): PageView => ({
    ...todayPage,
    date: addDays(today, offset),
    pageNumber: number,
    headline,
    dek,
    needsYou: [],
    changed: [],
    letGo: [],
    question: null,
    variant: "quiet",
    keptCount: 0,
    changesCount: 0,
    letGoCount: 0,
    pending: 0,
    tidiedFrom: at(offset, 2, 10),
    tidiedTo: at(offset, 2, 14),
  });

  const tripDay = addDays(friday, 6);

  const firstPage: PageView = {
    ...earlier(
      -13,
      1,
      "Your first page.",
      "Yesterday you kept one thing. Here is what Nocturne made of it overnight.",
    ),
    variant: "first",
    learned: [
      line(
        "line-learned-trip",
        "learned",
        `Willow Primary’s Year 4 trip is on **${formatDayLabel(tripDay)}**.`,
        ["mem-trip"],
        [receipt("item-letter")],
      ),
      line(
        "line-learned-form",
        "learned",
        `The form is due **${formatDayLabel(friday)}**.`,
        ["mem-trip"],
        [receipt("item-letter")],
      ),
      line(
        "line-learned-cost",
        "learned",
        "It costs **£12**.",
        ["mem-trip"],
        [receipt("item-letter")],
      ),
    ],
    question: {
      kind: "pageTime",
      text: "07:00",
      memoryId: null,
      receipt: null,
      lastAsked: null,
      answered: null,
    },
    keptCount: 1,
    changesCount: 0,
    letGoCount: 0,
  };

  const quietPage: PageView = {
    ...earlier(
      -8,
      18,
      "A quiet night. Nothing needs you.",
      "Nothing new was kept yesterday, so here is what is coming up instead.",
    ),
    upcoming: [
      line(
        "line-up-swim",
        "upcoming",
        "Leo’s swimming, 17:30",
        ["mem-swim"],
        [receipt("item-swim")],
        {
          on: nextMonday,
        },
      ),
      line(
        "line-up-dentist",
        "upcoming",
        "Dentist, 10:00",
        ["mem-dentist-thu"],
        [receipt("item-dentist-voice")],
        {
          on: thursday,
        },
      ),
      line("line-up-trip", "upcoming", "Trip form due", ["mem-trip"], [receipt("item-letter")], {
        on: friday,
      }),
    ],
    question: {
      kind: "yesNo",
      text: "Is Gran still at Rowan Court?",
      memoryId: "mem-gran-court",
      receipt: receipt("item-gran-court"),
      lastAsked: null,
      answered: null,
    },
  };

  const heavyPage: PageView = {
    ...earlier(
      -9,
      14,
      "Seven things need you. Three are today.",
      "A busy day: 62 things kept. Only what matters most is here.",
    ),
    variant: "heavy",
    needsYou: [
      line(
        "line-heavy-1",
        "promise",
        "Send Mrs Okafor the signed trip form, as you promised.",
        ["mem-trip"],
        [receipt("item-letter")],
      ),
      line(
        "line-heavy-2",
        "due",
        "Pay the parking permit, it ends today.",
        [],
        [receipt("item-parking")],
      ),
      line(
        "line-heavy-3",
        "due",
        "Book the Lake District cottage before Friday.",
        ["mem-lake"],
        [receipt("item-lake", { type: "time", startMs: 0, endMs: 42000 })],
      ),
    ],
    moreNeedsYou: 4,
    changed: [
      line(
        "line-heavy-4",
        "changed",
        "Leo’s swimming moved to **Monday**.",
        ["mem-swim"],
        [receipt("item-swim")],
      ),
      line(
        "line-heavy-5",
        "changed",
        "Gran’s new number ends **4471**.",
        ["mem-gran"],
        [receipt("item-gran")],
      ),
    ],
    moreChanged: 16,
    keptCount: 62,
    changesCount: 19,
    question: null,
  };

  const writingPage: PageView = {
    ...earlier(-7, 17, "Your page is being written.", "Nocturne is still reading what you kept."),
    variant: "beingWritten",
    needsYou: [
      line(
        "line-writing-1",
        "due",
        "The Year 4 trip form needs a signature and £12.",
        ["mem-trip"],
        [receipt("item-letter")],
      ),
    ],
    pending: 6,
    keptCount: 14,
    question: null,
  };

  const catchUpPage: PageView = {
    ...earlier(
      -6,
      16,
      "Two nights in one page.",
      "The computer was off for two nights, so this page covers both.",
    ),
    variant: "catchUp",
    catchUpDays: 2,
    changed: [
      line(
        "line-catch-1",
        "changed",
        "Leo’s swimming moved to **Monday**.",
        ["mem-swim"],
        [receipt("item-swim")],
      ),
    ],
    changesCount: 1,
    keptCount: 3,
    question: null,
  };

  const pages: PageView[] = [
    todayPage,
    firstPage,
    quietPage,
    heavyPage,
    writingPage,
    catchUpPage,
    {
      ...earlier(
        -1,
        22,
        "One thing needs you today.",
        "Overnight Nocturne read 5 things you kept and noticed 2 changes.",
      ),
      variant: "normal",
      needsYou: [
        line(
          "line-old-letter",
          "due",
          "The Year 4 trip form needs a signature and £12.",
          ["mem-trip"],
          [receipt("item-letter")],
        ),
      ],
      keptCount: 5,
      changesCount: 2,
    },
    earlier(-2, 21, "A quiet night. Nothing needs you.", "Nothing new was kept yesterday."),
    earlier(-3, 20, "A quiet night. Nothing needs you.", "Nothing new was kept yesterday."),
  ];

  return pages;
}
