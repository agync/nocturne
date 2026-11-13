import { addDays, formatDay, localDay } from "../../copy/format";
import type {
  EntityView,
  ItemKind,
  ItemView,
  MemoryView,
  PageLine,
  PageView,
  Question,
  Receipt,
  SettingsView,
  Source,
  Span,
} from "../bindings";

export const DEFAULT_NOW = "2026-11-13T07:02:00Z";

/** Predicates that make a memory a safety memory: kept forever, never the daily question. */
export const SAFETY_PREDICATES: ReadonlySet<string> = new Set([
  "takes",
  "allergicTo",
  "allergy",
  "medication",
  "diagnosedWith",
  "condition",
]);

export interface SeedData {
  items: ItemView[];
  memories: MemoryView[];
  entities: EntityView[];
  pages: PageView[];
  settings: SettingsView;
}

const formatDayLabel = formatDay;

const longWeekday = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { weekday: "long", day: "numeric", month: "short" })
    .format(new Date(`${day}T12:00:00`))
    .replace(/,/g, "");

/** The day offset to the next given weekday (0 = Sunday), at least one day ahead. */
function nextWeekday(today: string, weekday: number): number {
  const current = new Date(`${today}T12:00:00`).getDay();
  const delta = (weekday - current + 7) % 7;
  return delta === 0 ? 7 : delta;
}

export function buildSeed(now: Date): SeedData {
  const today = localDay(now);
  const at = (offset: number, hh: number, mm = 0): string => {
    const day = addDays(today, offset);
    const [y, m, d] = day.split("-").map(Number) as [number, number, number];
    return new Date(y, m - 1, d, hh, mm).toISOString();
  };

  const item = (
    id: string,
    kind: ItemKind,
    title: string,
    source: Source,
    keptAt: string,
    extra: Partial<ItemView> = {},
  ): ItemView => ({
    id,
    kind,
    title,
    source,
    trust: source === "pastedWeb" || source === "agent" ? "untrusted" : "trusted",
    keptAt,
    status: "understood",
    reason: null,
    durationMs: null,
    pages: null,
    fileName: null,
    hasOriginal: true,
    private: false,
    memoryCount: 1,
    understoodInMs: 4000,
    summary: null,
    text: null,
    language: "en",
    ...extra,
  });

  const items: ItemView[] = [
    item("item-letter", "image", "Letter", "drop", at(-2, 18, 12), {
      summary:
        "A letter from Willow Primary about the Year 4 trip: signature and £12 by the deadline.",
      memoryCount: 2,
    }),
    item("item-card", "image", "Appointment card", "drop", at(-10, 9, 30), {
      summary: "An appointment card from the dentist.",
    }),
    item("item-dentist-voice", "audio", "Dentist moved", "recording", at(-3, 12, 40), {
      durationMs: 8000,
      text: "They moved me to Thursday at ten.",
    }),
    item("item-swim", "text", "Swimming moved to Monday", "paste", at(-1, 17, 5), {
      text: "Swimming is now Mondays at 17:30, same pool.",
    }),
    item("item-lake", "audio", "Lake District", "recording", at(-1, 19, 22), {
      durationMs: 42000,
      text: "Let’s book the Lake District cottage for half term.",
    }),
    item("item-gran", "text", "Gran’s new number", "quickCapture", at(-1, 20, 1), {
      text: "Gran’s new number ends 4471.",
    }),
    item("item-coach", "text", "Swimming with Coach Sam", "quickCapture", at(-9, 10, 15), {
      text: "Leo swims with Coach Sam on Wednesdays.",
    }),
    item("item-gym", "text", "Gym door code", "quickCapture", at(-95, 8, 0), {
      text: "Gym door code is 4821.",
    }),
    item("item-pizza", "text", "Pizza place on Elm Street", "quickCapture", at(-120, 19, 0), {
      text: "Good pizza place on Elm Street.",
    }),
    item("item-gp", "image", "GP letter", "drop", at(-250, 11, 0), {
      summary: "A letter from the GP about a penicillin allergy.",
    }),
    item("item-prescription", "image", "Prescription", "drop", at(-3, 15, 0), {
      summary: "A prescription for Gran: amlodipine 2.5 mg, once a day.",
    }),
    item("item-nut", "web", "Nut-free lunchbox ideas", "pastedWeb", at(-4, 13, 0), {
      status: "unconfirmed",
      text: "Leo is allergic to nuts so I always...",
    }),
    item("item-dentist-chen", "text", "Dr Chen", "quickCapture", at(-30, 9, 0), {
      text: "Leo’s dentist is Dr Chen at Willow Dental.",
    }),
    item("item-counciltax", "document", "council-tax-2026.pdf", "drop", at(-1, 9, 9), {
      status: "couldntRead",
      reason: "locked",
      fileName: "council-tax-2026.pdf",
      pages: 4,
      memoryCount: 0,
      understoodInMs: null,
    }),
    item("item-gran-court", "text", "Where Gran lives", "quickCapture", at(-40, 11, 0), {
      text: "Gran lives at Rowan Court.",
    }),
    item("item-jo", "text", "Swimming teacher", "quickCapture", at(-66, 16, 0), {
      text: "Ava’s swimming teacher is called Jo.",
    }),
    item("item-bin", "text", "Bin day", "paste", at(-84, 8, 0), {
      text: "Bin day moved to Tuesdays over summer.",
    }),
    item("item-parking", "image", "Parking permit renewal", "quickCapture", at(0, 6, 41), {
      status: "reading",
      memoryCount: 0,
      understoodInMs: null,
    }),
    item("item-sam", "audio", "Ask Sam about Saturday", "recording", at(0, 6, 15), {
      durationMs: 6000,
      text: "Ask Sam about Saturday.",
    }),
    item("item-whatsapp", "text", "Class WhatsApp, 3 messages", "paste", at(0, 5, 58), {
      text: "Class WhatsApp: reminders about swimming kit and the school trip.",
    }),
  ];

  const byId = new Map(items.map((i) => [i.id, i]));
  const receipt = (itemId: string, span: Span | null = null, label?: string): Receipt => {
    const found = byId.get(itemId);
    if (!found) throw new Error(`Seed receipt points at a missing item: ${itemId}`);
    return {
      itemId,
      kind: found.kind,
      source: found.source,
      label: label ?? found.title,
      keptAt: found.keptAt,
      durationMs: found.durationMs,
      span,
      unconfirmed: found.status === "unconfirmed",
    };
  };

  const tuesday = addDays(today, nextWeekday(today, 2));
  const thursday = addDays(today, nextWeekday(today, 4));
  const friday = addDays(today, nextWeekday(today, 5));
  const tuesdayText = `${longWeekday(tuesday)}, 14:00`;
  const thursdayText = `${longWeekday(thursday)}, 10:00`;

  const memory = (
    id: string,
    sentence: string,
    subject: string,
    predicate: string,
    object: string,
    receipts: Receipt[],
    extra: Partial<MemoryView> = {},
  ): MemoryView => ({
    id,
    sentence,
    subject,
    entityId: null,
    predicate,
    object,
    certainty: "fairlySure",
    status: "active",
    validFrom: null,
    validTo: null,
    due: null,
    learnedAt: receipts[0]?.keptAt ?? at(0, 0),
    lastUsed: null,
    letGoAt: null,
    keepForever: SAFETY_PREDICATES.has(predicate),
    private: false,
    suggestPrivate: false,
    was: null,
    receipts,
    ...extra,
  });

  const memories: MemoryView[] = [
    memory(
      "mem-swim",
      "Leo’s swimming is on Mondays at 17:30.",
      "Leo",
      "swimsOn",
      "Monday 17:30",
      [receipt("item-swim")],
      { entityId: "ent-leo", was: "Wednesday", certainty: "sure", lastUsed: at(-1, 17, 5) },
    ),
    memory(
      "mem-lake",
      "You decided to book the Lake District cottage for half term.",
      "You",
      "decided",
      "Lake District cottage",
      [receipt("item-lake", { type: "time", startMs: 0, endMs: 42000 })],
    ),
    memory(
      "mem-gran",
      "Gran’s new number ends 4471.",
      "Gran",
      "phoneEndsWith",
      "4471",
      [receipt("item-gran")],
      { entityId: "ent-gran", certainty: "sure" },
    ),
    memory(
      "mem-trip",
      "The Year 4 trip form is due next Friday. It needs a signature and £12.",
      "Year 4 trip form",
      "dueOn",
      friday,
      [receipt("item-letter")],
      { due: friday, entityId: "ent-leo" },
    ),
    memory(
      "mem-dentist-tue",
      `Your dentist appointment is ${tuesdayText}.`,
      "Dentist appointment",
      "appointmentAt",
      tuesdayText,
      [receipt("item-card")],
      { status: "unconfirmed", certainty: "notSure" },
    ),
    memory(
      "mem-dentist-thu",
      `Your dentist appointment is ${thursdayText}.`,
      "Dentist appointment",
      "appointmentAt",
      thursdayText,
      [receipt("item-dentist-voice", { type: "time", startMs: 0, endMs: 8000 })],
      { status: "unconfirmed", certainty: "notSure" },
    ),
    memory(
      "mem-dentist-chen",
      "Leo’s dentist is Dr Chen at Willow Dental.",
      "Dr Chen",
      "dentistFor",
      "Leo",
      [receipt("item-dentist-chen")],
      { entityId: "ent-chen", certainty: "sure" },
    ),
    memory(
      "mem-coach",
      "Leo swims with Coach Sam.",
      "Leo",
      "swimsWith",
      "Coach Sam",
      [receipt("item-coach")],
      { entityId: "ent-leo", lastUsed: at(-9, 10, 15) },
    ),
    memory(
      "mem-gym",
      "The old gym door code is 4821.",
      "Gym door code",
      "isCode",
      "4821",
      [receipt("item-gym")],
      { status: "letGo", letGoAt: at(0, 2, 10), lastUsed: at(-90, 8, 0) },
    ),
    memory(
      "mem-pizza",
      "The pizza place on Elm Street is good.",
      "Pizza place",
      "isGood",
      "Elm Street",
      [receipt("item-pizza")],
      { status: "letGo", letGoAt: at(0, 2, 10), lastUsed: at(-120, 19, 0) },
    ),
    memory(
      "mem-nut",
      "Leo is allergic to nuts.",
      "Leo",
      "allergicTo",
      "nuts",
      [receipt("item-nut", { type: "quote", text: "Leo is allergic to nuts so I always..." })],
      { status: "unconfirmed", certainty: "notSure", entityId: "ent-leo", suggestPrivate: true },
    ),
    memory(
      "mem-ava-penicillin",
      "Ava is allergic to penicillin.",
      "Ava",
      "allergicTo",
      "penicillin",
      [receipt("item-gp")],
      { entityId: "ent-ava", certainty: "sure", suggestPrivate: true },
    ),
    memory(
      "mem-gran-tablets",
      "Gran takes amlodipine 2.5 mg once a day.",
      "Gran",
      "takes",
      "amlodipine 2.5 mg",
      [receipt("item-prescription")],
      { entityId: "ent-gran", certainty: "sure", private: true, suggestPrivate: true },
    ),
    memory(
      "mem-gran-court",
      "Gran lives at Rowan Court.",
      "Gran",
      "livesAt",
      "Rowan Court",
      [receipt("item-gran-court")],
      { entityId: "ent-gran", lastUsed: at(-40, 11, 0) },
    ),
    memory(
      "mem-jo",
      "Ava’s old swimming teacher is called Jo.",
      "Ava",
      "teacherIs",
      "Jo",
      [receipt("item-jo")],
      { status: "fading", entityId: "ent-ava", lastUsed: at(-67, 16, 0), certainty: "notSure" },
    ),
    memory(
      "mem-bin",
      "Bin day moved to Tuesdays over summer.",
      "Bin day",
      "movedTo",
      "Tuesdays",
      [receipt("item-bin")],
      { status: "fading", lastUsed: at(-84, 8, 0), certainty: "notSure" },
    ),
  ];

  const entities: EntityView[] = [
    { id: "ent-leo", kind: "person", name: "Leo", role: "Son", memoryCount: 5, itemCount: 5 },
    { id: "ent-ava", kind: "person", name: "Ava", role: "Daughter", memoryCount: 2, itemCount: 2 },
    {
      id: "ent-gran",
      kind: "person",
      name: "Gran (Margaret)",
      role: "Mother",
      memoryCount: 3,
      itemCount: 3,
    },
    {
      id: "ent-okafor",
      kind: "person",
      name: "Mrs Okafor",
      role: "Leo’s teacher",
      memoryCount: 1,
      itemCount: 1,
    },
    { id: "ent-sam", kind: "person", name: "Sam", role: "Partner", memoryCount: 2, itemCount: 2 },
    {
      id: "ent-chen",
      kind: "person",
      name: "Dr Chen",
      role: "Dentist",
      memoryCount: 1,
      itemCount: 1,
    },
  ];

  const mem = (id: string) => {
    const found = memories.find((m) => m.id === id);
    if (!found) throw new Error(`Seed memory missing: ${id}`);
    return found;
  };

  const line = (
    id: string,
    kind: PageLine["kind"],
    text: string,
    memoryIds: string[],
    receipts: Receipt[],
    extra: Partial<PageLine> = {},
  ): PageLine => ({
    id,
    kind,
    text,
    memoryIds,
    receipts,
    choices: [],
    conflictId: null,
    done: false,
    forgotten: false,
    on: null,
    lastUsed: null,
    ...extra,
  });

  const question: Question = {
    kind: "yesNo",
    text: "Is Leo still doing swimming with Coach Sam?",
    memoryId: "mem-coach",
    receipt: mem("mem-coach").receipts[0] ?? null,
    lastAsked: null,
    answered: null,
  };

  const todayPage: PageView = {
    date: today,
    pageNumber: 23,
    edition: "screen",
    variant: "normal",
    headline: "Two things need you today.",
    dek: "Overnight Nocturne read 14 things you kept, noticed 3 changes and let 3 go.",
    needsYou: [
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
    ],
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
      line("line-gym", "letGo", "The old gym door code", ["mem-gym"], [], {
        lastUsed: addDays(today, -90),
      }),
      line("line-pizza", "letGo", "The pizza place on Elm Street", ["mem-pizza"], [], {
        lastUsed: addDays(today, -120),
      }),
      line("line-bin", "merged", "Two notes about bin day were merged into one", [], []),
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
      text: "Is this page arriving at a good time?",
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
          on: addDays(today, nextWeekday(today, 1)),
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

  const settings: SettingsView = {
    pageTime: "07:00",
    delivery: "notify",
    autoPrint: false,
    voice: "isla",
    voiceSpeed: 1,
    shortcut: "Ctrl+Shift+Space",
    shortcutAvailable: true,
    inboxFolder: null,
    startAtLogin: true,
    lettingGo: "gentle",
    theme: "system",
    language: "en-GB",
    checkUpdates: false,
    reminders: false,
    lock: false,
    agents: false,
  };

  return { items, memories, entities, pages, settings };
}
