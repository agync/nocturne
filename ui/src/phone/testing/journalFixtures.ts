// The journal as the mockups show it: Thursday 19 November, with Wednesday looked back on and
// the week before. The scripted phone engine and the fake computer both answer from here.

import type {
  CheckIn,
  DayCount,
  DayReflection,
  EntityKind,
  Feeling,
  FeelingPart,
  GoodThing,
  ItemKind,
  ItemView,
  JournalDay,
  Mention,
  NameRef,
  Period,
  Recap,
  Receipt,
  ReflectionView,
  RowKind,
  SameNameQuestion,
  TimelineRow,
} from "../../engine/bindings";

export const TODAY = "2026-11-19";
export const YESTERDAY = "2026-11-18";

export const DAY_PERIOD: Period = { kind: "day", start: TODAY, end: TODAY };
export const YESTERDAY_PERIOD: Period = { kind: "day", start: YESTERDAY, end: YESTERDAY };
export const WEEK_PERIOD: Period = { kind: "week", start: "2026-11-09", end: "2026-11-15" };

const at = (day: string, time: string) => `${day}T${time}:00.000Z`;

type ItemOptions = Partial<ItemView> & { id: string; kind: ItemKind; keptAt: string };

export function itemView(o: ItemOptions): ItemView {
  return {
    title: o.text?.slice(0, 40) ?? o.id,
    source: "journal",
    trust: "trusted",
    status: "understood",
    reason: null,
    durationMs: null,
    pages: null,
    fileName: null,
    hasOriginal: false,
    originalPath: null,
    private: false,
    memoryCount: 0,
    understoodInMs: null,
    summary: null,
    text: null,
    language: null,
    fromDevice: null,
    ...o,
  };
}

function mention(itemId: string, name: string, kind: EntityKind, role: string | null): Mention {
  return {
    id: `men-${itemId}-${name}`,
    itemId,
    entityId: `ent-${name.toLowerCase()}`,
    name,
    kind,
    role,
    start: null,
    end: null,
    state: "found",
    new: false,
    memoryCount: 0,
  };
}

const feeling = (
  part: FeelingPart,
  pleasant: number,
  energy: number,
  words: string[],
): Feeling => ({ part, point: { pleasant, energy }, words });

export function checkIn(
  id: string,
  day: string,
  time: string,
  f: Feeling,
  extra: Partial<CheckIn> = {},
): CheckIn {
  return {
    id,
    keptAt: at(day, time),
    day,
    feeling: f,
    line: null,
    urge: null,
    about: null,
    tried: [],
    fromDevice: null,
    mentions: [],
    leftOut: false,
    ...extra,
  };
}

const checkinItem = (c: CheckIn): ItemView =>
  itemView({
    id: c.id,
    kind: "checkin",
    keptAt: c.keptAt,
    title: c.feeling.words.join(", "),
    text: c.line,
    journal: "checkin",
  });

function row(
  kind: RowKind,
  part: TimelineRow["part"],
  item: ItemView,
  extra: Partial<TimelineRow>,
) {
  return {
    kind,
    part,
    item,
    checkin: null,
    good: null,
    feeling: null,
    mentions: [],
    leftOut: false,
    moveTo: null,
    ...extra,
  } satisfies TimelineRow;
}

function good(id: string, day: string, time: string, text: string): GoodThing {
  return {
    id,
    text,
    day,
    keptAt: at(day, time),
    photo: null,
    fromDevice: null,
    mentions: [],
    leftOut: false,
  };
}

function name(
  entity: string,
  display: string,
  kind: EntityKind,
  itemIds: string[],
  role: string | null = null,
): NameRef {
  return { entityId: `ent-${entity}`, name: display, kind, role, new: false, itemIds };
}

// Today -------------------------------------------------------------------------------------

const ENTRY_TEXT =
  "Finally had lunch with Priya. We sat outside the café on Mill Lane even though it was cold. " +
  "Leo forgot his PE kit again, so I dropped it at the school office on the way to work.";

const NOTE_PE = itemView({
  id: "item-pe",
  kind: "text",
  source: "phone",
  keptAt: at(TODAY, "07:52"),
  text: "Leo forgot his PE kit again. Dropped it at the school office on the way to work.",
  fromDevice: "Pixel",
});
const NOTE_LUNCH = itemView({
  id: "item-lunch",
  kind: "text",
  source: "phone",
  keptAt: at(TODAY, "13:12"),
  text: "Lunch with Priya, finally",
  fromDevice: "Pixel",
});
const NOTE_TOMATOES = itemView({
  id: "item-tomatoes",
  kind: "image",
  source: "phone",
  keptAt: at(TODAY, "15:40"),
  title: "Tomatoes ripening on a windowsill.",
  hasOriginal: true,
  fromDevice: "Pixel",
});
const DOC_FORM = itemView({
  id: "item-form",
  kind: "document",
  source: "drop",
  keptAt: at(TODAY, "09:30"),
  title: "Year 4 trip form",
  hasOriginal: true,
});
const NOTE_SAM = itemView({
  id: "item-sam",
  kind: "text",
  source: "phone",
  keptAt: at(TODAY, "13:20"),
  text: "I think I’ll go with her and Sam on Saturday.",
  fromDevice: "Pixel",
});

const CHECKIN_TIRED = checkIn(
  "checkin-tired",
  TODAY,
  "08:05",
  feeling("low", -0.5, -0.5, ["tired"]),
  {
    line: "Bad night.",
  },
);
const CHECKIN_PLEASED = checkIn(
  "checkin-pleased",
  TODAY,
  "13:12",
  feeling("litUp", 0.7, 0.4, ["pleased"]),
  { about: NOTE_LUNCH.id, fromDevice: "Pixel" },
);
const CHECKIN_RELIEVED = checkIn(
  "checkin-relieved",
  TODAY,
  "18:10",
  feeling("settled", 0.6, -0.4, ["relieved", "content"]),
  {
    line: "Gran’s scan came back clear.",
    mentions: [mention("checkin-relieved", "Gran", "person", "your grandmother")],
  },
);
const GOOD_GATE = good("good-gate", TODAY, "19:30", "Sam fixed the gate without being asked.");

export const SAME_NAME: SameNameQuestion = {
  id: "snq-0001",
  name: "Sam",
  quote: "I think I’ll go with her and Sam on Saturday.",
  receipt: receiptFor(NOTE_SAM, "Note"),
  options: [
    { entityId: "ent-sam-partner", name: "Sam", role: "your partner" },
    { entityId: "ent-sam-work", name: "Sam", role: "from work" },
  ],
};

function receiptFor(item: ItemView, label: string): Receipt {
  return {
    itemId: item.id,
    kind: item.kind,
    source: item.source,
    label,
    keptAt: item.keptAt,
    durationMs: null,
    span: null,
    unconfirmed: false,
    fromDevice: item.fromDevice,
  };
}

export function todayDay(): JournalDay {
  return {
    day: TODAY,
    today: true,
    firstUse: false,
    entry: [
      {
        item: itemView({
          id: "item-entry-1",
          kind: "text",
          keptAt: at(TODAY, "21:02"),
          text: ENTRY_TEXT,
          journal: "entry",
          status: "kept",
        }),
        block: 0,
        mentions: [mention("item-entry-1", "Priya", "person", null)],
        earlier: [],
        changedAt: null,
        alt: null,
      },
    ],
    entrySavedAt: at(TODAY, "21:02"),
    entryReadAt: null,
    rows: [
      row("note", "morning", NOTE_PE, {
        mentions: [mention(NOTE_PE.id, "Leo", "person", "your son")],
      }),
      row("checkin", "morning", checkinItem(CHECKIN_TIRED), { checkin: CHECKIN_TIRED }),
      row("elsewhere", "morning", DOC_FORM, {}),
      row("note", "afternoon", NOTE_LUNCH, { feeling: CHECKIN_PLEASED }),
      row("note", "afternoon", NOTE_SAM, {}),
      row("photo", "afternoon", NOTE_TOMATOES, {}),
      row("checkin", "evening", checkinItem(CHECKIN_RELIEVED), { checkin: CHECKIN_RELIEVED }),
      row(
        "good",
        "evening",
        itemView({
          id: GOOD_GATE.id,
          kind: "text",
          keptAt: GOOD_GATE.keptAt,
          text: GOOD_GATE.text,
          journal: "good",
        }),
        { good: GOOD_GATE },
      ),
    ],
    checkins: [CHECKIN_TIRED, CHECKIN_PLEASED, CHECKIN_RELIEVED],
    good: [GOOD_GATE],
    people: [
      name("leo", "Leo", "person", [NOTE_PE.id], "your son"),
      name("gran", "Gran", "person", [CHECKIN_RELIEVED.id], "your grandmother"),
      name("priya", "Priya", "person", ["item-entry-1"]),
    ],
    reflection: null,
    reading: 1,
    sameName: [SAME_NAME],
  };
}

// Yesterday ---------------------------------------------------------------------------------

const Y_NOTE = itemView({
  id: "item-y-note",
  kind: "text",
  source: "quickCapture",
  keptAt: at(YESTERDAY, "17:10"),
  text: "Leo swimming, straight from school. Sam cooking for once.",
});
const Y_VOICE = itemView({
  id: "item-y-voice",
  kind: "audio",
  source: "phone",
  keptAt: at(YESTERDAY, "19:02"),
  title: "Dentist’s new time",
  text: "The dentist moved my appointment to Tuesday at 8:40.",
  durationMs: 31_000,
  hasOriginal: true,
  fromDevice: "Pixel",
});
const Y_STRESSED = checkIn(
  "checkin-y-stressed",
  YESTERDAY,
  "16:30",
  feeling("woundUp", -0.6, 0.6, ["stressed"]),
  { line: "Running late for swimming." },
);
const Y_CONTENT = checkIn(
  "checkin-y-content",
  YESTERDAY,
  "21:10",
  feeling("settled", 0.6, -0.3, ["content"]),
);
const Y_GOOD = good("good-y-length", YESTERDAY, "18:00", "Leo swam a whole length.");

const YESTERDAY_REFLECTION: DayReflection = {
  id: "ref-y",
  period: YESTERDAY_PERIOD,
  writtenAt: at(TODAY, "02:10"),
  written: "overnight",
  soFar: false,
  fromCount: 7,
  theDay: [
    {
      text: "You took **Leo to swimming** straight from school, and **Sam cooked** for once. You kept the **dentist’s new time**: Tuesday at 8:40.",
      receipts: [
        receiptFor(Y_NOTE, "Note"),
        receiptFor(Y_NOTE, "Note"),
        receiptFor(Y_VOICE, "Voice note"),
      ],
    },
  ],
  felt: [Y_STRESSED, Y_CONTENT],
  good: [Y_GOOD],
  people: [
    name("leo", "Leo", "person", [Y_NOTE.id]),
    name("sam", "Sam", "person", [Y_NOTE.id]),
    name("leisure", "the leisure centre", "place", [Y_NOTE.id]),
  ],
  stillOpen: [],
  stillReading: 0,
  linesRemoved: 0,
};

export function yesterdayDay(): JournalDay {
  return {
    ...emptyDay(YESTERDAY, false),
    rows: [
      row("checkin", "afternoon", checkinItem(Y_STRESSED), { checkin: Y_STRESSED }),
      row("note", "afternoon", Y_NOTE, {}),
      row(
        "good",
        "afternoon",
        itemView({
          id: Y_GOOD.id,
          kind: "text",
          keptAt: Y_GOOD.keptAt,
          text: Y_GOOD.text,
          journal: "good",
        }),
        { good: Y_GOOD },
      ),
      row("voice", "evening", Y_VOICE, {}),
      row("checkin", "evening", checkinItem(Y_CONTENT), { checkin: Y_CONTENT }),
    ],
    checkins: [Y_STRESSED, Y_CONTENT],
    good: [Y_GOOD],
    people: YESTERDAY_REFLECTION.people,
    reflection: {
      firstSentence: "You took Leo to swimming straight from school, and Sam cooked for once.",
      writtenAt: YESTERDAY_REFLECTION.writtenAt,
      fromCount: 7,
    },
  };
}

export function emptyDay(day: string, today: boolean): JournalDay {
  return {
    day,
    today,
    firstUse: false,
    entry: [],
    entrySavedAt: null,
    entryReadAt: null,
    rows: [],
    checkins: [],
    good: [],
    people: [],
    reflection: null,
    reading: 0,
    sameName: [],
  };
}

// Looking back ------------------------------------------------------------------------------

const WEEK_CHECKINS: CheckIn[] = [
  ["2026-11-09", "tired", "low"],
  ["2026-11-10", "tired", "low"],
  ["2026-11-11", "stressed", "woundUp"],
  ["2026-11-12", "content", "settled"],
  ["2026-11-13", "stressed", "woundUp"],
  ["2026-11-14", "proud", "litUp"],
  ["2026-11-15", "content", "settled"],
].map(([day, word, part], i) =>
  checkIn(
    `checkin-w-${i}`,
    day as string,
    "20:00",
    feeling(part as FeelingPart, part === "woundUp" || part === "low" ? -0.5 : 0.5, 0, [
      word as string,
    ]),
  ),
);

const WEEK_RECAP: Recap = {
  id: "recap-week",
  period: WEEK_PERIOD,
  writtenAt: at("2026-11-16", "02:20"),
  written: "overnight",
  soFar: false,
  days: 7,
  highlights: [
    {
      day: "2026-11-10",
      text: "Leo read the whole of his school play part to Gran on the phone.",
      quoted: true,
      receipts: [],
      photo: null,
      feeling: null,
    },
    {
      day: "2026-11-14",
      text: "First frost on the allotment. Dug the last of the leeks with Gran.",
      quoted: true,
      receipts: [],
      photo: null,
      feeling: null,
    },
    {
      day: "2026-11-15",
      text: "You decided to **take Fridays off in December**.",
      quoted: false,
      receipts: [],
      photo: null,
      feeling: null,
    },
  ],
  checkins: WEEK_CHECKINS,
  words: [
    { word: "tired", part: "low", checkinIds: ["checkin-w-0", "checkin-w-1", "checkin-w-2"] },
    { word: "stressed", part: "woundUp", checkinIds: ["checkin-w-2", "checkin-w-4"] },
    { word: "content", part: "settled", checkinIds: ["checkin-w-3", "checkin-w-6"] },
    { word: "proud", part: "litUp", checkinIds: ["checkin-w-5"] },
  ],
  together: [
    {
      count: 3,
      total: 4,
      with: name("gran", "Gran", "person", []),
      part: "settled",
      word: null,
      receipts: [],
    },
  ],
  compare: null,
  good: [
    good("good-w-1", "2026-11-10", "18:00", "Leo’s play rehearsal."),
    good("good-w-2", "2026-11-14", "10:00", "Frost on the allotment."),
  ],
  people: [
    ["leo", "Leo", "person", 7],
    ["gran", "Gran", "person", 4],
    ["allotment", "the allotment", "place", 2],
  ].map(
    ([id, display, kind, days]): DayCount => ({
      with: name(id as string, display as string, kind as EntityKind, []),
      days: Array.from(
        { length: days as number },
        (_, i) => `2026-11-${String(9 + i).padStart(2, "0")}`,
      ),
    }),
  ),
  newNames: [],
  tried: [],
  changed: [],
  stillOpen: [
    {
      id: "mem-eye-test",
      sentence: "You said you would book Gran’s eye test.",
      subject: "Gran",
      entityId: "ent-gran",
      predicate: "promised",
      object: "Gran’s eye test",
      certainty: "sure",
      status: "active",
      validFrom: null,
      validTo: null,
      due: null,
      learnedAt: at("2026-11-09", "09:00"),
      lastUsed: null,
      letGoAt: null,
      keepForever: false,
      private: false,
      suggestPrivate: false,
      was: null,
      movedFrom: null,
      receipts: [],
    },
  ],
  linesRemoved: 0,
};

export function lookBackFor(period: Period): ReflectionView {
  if (period.kind === "day" && period.start === YESTERDAY) {
    return { type: "written", reflection: YESTERDAY_REFLECTION };
  }
  if (period.kind === "week" && period.start === WEEK_PERIOD.start) {
    return { type: "recap", recap: WEEK_RECAP };
  }
  if (period.kind === "day" && period.start === TODAY) {
    return { type: "notYet", glance: todayGlance() };
  }
  return { type: "nothing", period };
}

export function todayGlance() {
  return {
    period: DAY_PERIOD,
    kept: [
      { kind: "note" as const, itemIds: [NOTE_PE.id, NOTE_LUNCH.id, NOTE_SAM.id] },
      { kind: "photo" as const, itemIds: [NOTE_TOMATOES.id] },
      { kind: "document" as const, itemIds: [DOC_FORM.id] },
    ],
    felt: [CHECKIN_TIRED, CHECKIN_PLEASED, CHECKIN_RELIEVED],
    good: [GOOD_GATE],
    with: todayDay().people,
    entryLines: ["Finally had lunch with Priya.", "We sat outside the café on Mill Lane."],
    entryItemId: "item-entry-1",
  };
}

/** What "Reflect on today" writes while the day is still going. */
export function todayReflection(writtenAt: string): DayReflection {
  return {
    id: "ref-today",
    period: DAY_PERIOD,
    writtenAt,
    written: "onDemand",
    soFar: true,
    fromCount: 8,
    theDay: [
      {
        text: "You had **lunch with Priya** outside the café, and **Sam fixed the gate**.",
        receipts: [receiptFor(NOTE_LUNCH, "Note"), receiptFor(NOTE_SAM, "Note")],
      },
    ],
    felt: [CHECKIN_TIRED, CHECKIN_PLEASED, CHECKIN_RELIEVED],
    good: [GOOD_GATE],
    people: todayDay().people,
    stillOpen: [],
    stillReading: 1,
    linesRemoved: 0,
  };
}

export { at as stamp, feeling as feelingFor };
