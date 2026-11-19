import { addDays, localDay } from "../../../copy/format";
import type {
  CheckIn,
  EntityKind,
  EntryBlock,
  Feeling,
  FeelingPart,
  GoodThing,
  ItemKind,
  ItemView,
  Mention,
  SameNameQuestion,
  Source,
  Span,
} from "../../bindings";

/** `?mockJournal=<state>`: which journal the mock starts from. */
export type JournalMode = "day" | "first" | "empty" | "partial" | "notready" | "glance" | "writing";

export const JOURNAL_MODES: readonly JournalMode[] = [
  "day",
  "first",
  "empty",
  "partial",
  "notready",
  "glance",
  "writing",
];

/** The journal day a moment belongs to: a day runs from 4 in the morning to 4 the next morning. */
export function journalDayOf(instant: Date | string): string {
  return localDay(new Date(new Date(instant).getTime() - 4 * 3_600_000));
}

/** A sentence for a reflection: `**spans**` each cite the item at the same index, or the record's own. */
export interface Says {
  text: string;
  ids?: string[];
  spans?: Span[];
}

export type Rec =
  | { t: "block"; day: string; block: EntryBlock; says?: Says }
  | {
      t: "note";
      day: string;
      itemId: string;
      row: "note" | "voice" | "photo" | "elsewhere";
      mentions: Mention[];
      leftOut: boolean;
      /** The check-in tapped with it. */
      feeling: string | null;
      moveTo: string | null;
      says?: Says;
    }
  | { t: "checkin"; day: string; checkin: CheckIn; says?: Says }
  | { t: "good"; day: string; good: GoodThing; says?: Says };

export interface KnownEntity {
  id: string;
  name: string;
  kind: EntityKind;
  role: string | null;
}

/** An open "still open" memory the day's reflection and the recap link to Needs you. */
export interface OpenThing {
  id: string;
  sentence: string;
  subject: string;
  day: string;
  itemId: string;
  due: string | null;
}

export interface JournalData {
  mode: JournalMode;
  today: string;
  firstUse: boolean;
  items: Map<string, ItemView>;
  recs: Rec[];
  mentions: Map<string, Mention>;
  entities: Map<string, KnownEntity>;
  questions: SameNameQuestion[];
  open: OpenThing[];
  /** Highlights by day, in the person's own words. */
  highlights: Map<string, { text: string; quoted: boolean; itemId: string; photoId?: string }>;
  /** Ideas tried, by check-in. */
  counter: number;
}

const ENTITIES: KnownEntity[] = [
  { id: "ent-gran", name: "Gran", kind: "person", role: "Mother" },
  { id: "ent-leo", name: "Leo", kind: "person", role: "Son" },
  { id: "ent-sam", name: "Sam", kind: "person", role: "your partner" },
  { id: "ent-sam-work", name: "Sam", kind: "person", role: "from work" },
  { id: "ent-priya", name: "Priya", kind: "person", role: null },
  { id: "ent-mill-lane", name: "Mill Lane café", kind: "place", role: null },
  { id: "ent-allotment", name: "the allotment", kind: "place", role: null },
  { id: "ent-climbing-wall", name: "the climbing wall", kind: "place", role: null },
  { id: "ent-school", name: "the school", kind: "place", role: null },
  { id: "ent-hendersons", name: "the Hendersons", kind: "person", role: "clients" },
  { id: "ent-fernhill", name: "Fernhill Primary hall", kind: "place", role: null },
];

const POINT: Record<FeelingPart, { pleasant: number; energy: number }> = {
  woundUp: { pleasant: -0.55, energy: 0.5 },
  litUp: { pleasant: 0.4, energy: 0.26 },
  low: { pleasant: -0.3, energy: -0.4 },
  settled: { pleasant: 0.5, energy: -0.44 },
};

export function feeling(part: FeelingPart, ...words: string[]): Feeling {
  return { part, point: { ...POINT[part] }, words };
}

const PART_BY_WORD: Record<string, FeelingPart> = {
  tired: "low",
  stressed: "woundUp",
  content: "settled",
  proud: "litUp",
  pleased: "litUp",
  relieved: "settled",
  worried: "woundUp",
  calm: "settled",
};

/** Builds the Thursday household and the weeks around it, relative to the mock clock. */
export function seedJournal(clock: Date, mode: JournalMode): JournalData {
  const today = journalDayOf(clock);
  const data: JournalData = {
    mode,
    today,
    firstUse: mode === "first",
    items: new Map(),
    recs: [],
    mentions: new Map(),
    entities: new Map(ENTITIES.map((e) => [e.id, e])),
    questions: [],
    open: [],
    highlights: new Map(),
    counter: 0,
  };
  const unread = mode === "partial" || mode === "notready";
  const at = (day: string, hh: number, mm = 0): string => {
    const [y, m, d] = day.split("-").map(Number) as [number, number, number];
    return new Date(y, m - 1, d, hh, mm).toISOString();
  };

  const item = (
    id: string,
    kind: ItemKind,
    day: string,
    hh: number,
    mm: number,
    extra: Partial<ItemView> = {},
  ): ItemView => {
    const made: ItemView = {
      id,
      kind,
      title: extra.text?.split("\n")[0]?.slice(0, 60) ?? "Note",
      source: "quickCapture" satisfies Source,
      trust: "trusted",
      keptAt: at(day, hh, mm),
      status: "understood",
      reason: null,
      durationMs: null,
      pages: null,
      fileName: null,
      hasOriginal: kind !== "text" && kind !== "checkin",
      originalPath: null,
      private: false,
      memoryCount: 0,
      understoodInMs: 2400,
      summary: null,
      text: null,
      language: null,
      fromDevice: null,
      ...extra,
    };
    data.items.set(id, made);
    return made;
  };

  const mention = (
    itemId: string,
    entityId: string,
    text: string | null,
    over: Partial<Mention> = {},
  ): Mention => {
    const known = data.entities.get(entityId);
    if (!known) throw new Error(`No entity ${entityId}`);
    data.counter += 1;
    const at0 = text === null ? -1 : text.indexOf(over.name ?? known.name);
    const name = over.name ?? known.name;
    const made: Mention = {
      id: `men-${data.counter}`,
      itemId,
      entityId,
      name,
      kind: known.kind,
      role: known.role,
      start: at0 >= 0 ? at0 : null,
      end: at0 >= 0 ? at0 + name.length : null,
      state: "found",
      new: false,
      memoryCount: 1,
      ...over,
    };
    data.mentions.set(made.id, made);
    return made;
  };

  /** Mentions are found by reading, so an item still unread has none. */
  const found = (
    itemId: string,
    text: string | null,
    list: Array<[string, Partial<Mention>?, string?]>,
  ): Mention[] =>
    unread
      ? []
      : list.map(([entityId, over, find]) =>
          mention(itemId, entityId, text, { ...over, ...(find ? { name: find } : {}) }),
        );

  const note = (
    id: string,
    day: string,
    hh: number,
    mm: number,
    text: string,
    people: Array<[string, Partial<Mention>?, string?]> = [],
    extra: Partial<ItemView> = {},
    rec: Partial<Extract<Rec, { t: "note" }>> = {},
  ) => {
    item(id, "text", day, hh, mm, { text, title: text.slice(0, 60), ...extra });
    data.recs.push({
      t: "note",
      day,
      itemId: id,
      row: "note",
      mentions: found(id, text, people),
      leftOut: false,
      feeling: null,
      moveTo: null,
      ...rec,
    });
  };

  const checkin = (
    id: string,
    day: string,
    hh: number,
    mm: number,
    f: Feeling,
    line: string | null,
    extra: Partial<CheckIn> = {},
    people: Array<[string, Partial<Mention>?, string?]> = [],
    says?: Says,
  ): CheckIn => {
    item(id, "checkin", day, hh, mm, {
      title: "Check-in",
      text: line,
      source: extra.fromDevice ? "phone" : "journal",
      fromDevice: extra.fromDevice ?? null,
      status: "understood",
    });
    const made: CheckIn = {
      id,
      keptAt: at(day, hh, mm),
      day,
      feeling: f,
      line,
      urge: null,
      about: null,
      tried: [],
      fromDevice: null,
      mentions: found(id, line, people),
      leftOut: false,
      ...extra,
    };
    data.recs.push({ t: "checkin", day, checkin: made, says });
    return made;
  };

  const good = (
    id: string,
    day: string,
    hh: number,
    mm: number,
    text: string,
    people: Array<[string, Partial<Mention>?, string?]> = [],
    says?: Says,
  ) => {
    item(id, "text", day, hh, mm, {
      text,
      title: text.slice(0, 60),
      journal: "good",
      source: "journal",
    });
    const made: GoodThing = {
      id,
      text,
      day,
      keptAt: at(day, hh, mm),
      photo: null,
      fromDevice: null,
      mentions: found(id, text, people),
      leftOut: false,
    };
    data.recs.push({ t: "good", day, good: made, says });
    return made;
  };

  let block = 0;
  const entryBlock = (
    id: string,
    day: string,
    hh: number,
    mm: number,
    kind: ItemKind,
    extra: Partial<ItemView>,
    people: Array<[string, Partial<Mention>?, string?]> = [],
    over: Partial<EntryBlock> = {},
    says?: Says,
  ) => {
    const view = item(id, kind, day, hh, mm, { journal: "entry", source: "journal", ...extra });
    const made: EntryBlock = {
      item: view,
      block: block++,
      mentions: found(id, view.text, people),
      earlier: [],
      changedAt: null,
      alt: null,
      ...over,
    };
    data.recs.push({ t: "block", day, block: made, says });
    return made;
  };

  // ---- Today: the mockups' Thursday ----
  const empty = mode === "first" || mode === "empty";
  if (!empty) {
    const paraOne =
      "Finally had lunch with Priya. We sat outside the café on Mill Lane even though it was cold, and she told me about the new climbing wall by the station. I think I’ll go with her and Sam on Saturday.";
    const paraTwo =
      "Gran rang at six. Her scan came back clear. She’d been at the allotment all afternoon and sounded tired but pleased with herself.";

    note(
      "j-note-leo",
      today,
      7,
      52,
      "Leo forgot his PE kit again. Dropped it at the school office on the way to work.",
      [["ent-leo"], ["ent-school", {}, "school"]],
      {},
      {
        says: {
          text: "**Leo forgot his PE kit again**, so you dropped it at the school office.",
        },
      },
    );
    checkin("j-ci-tired", today, 8, 5, feeling("low", "tired"), "Bad night.", {}, [], {
      text: "You were **tired after a bad night**.",
    });
    item("j-form", "document", today, 9, 30, {
      title: "Year 4 trip form",
      fileName: "Year 4 trip form.pdf",
      pages: 2,
      source: "watchedFolder",
      status: "understood",
      hasOriginal: true,
    });
    data.recs.push({
      t: "note",
      day: today,
      itemId: "j-form",
      row: "elsewhere",
      mentions: [],
      leftOut: false,
      feeling: null,
      moveTo: null,
    });

    const lunch = "Lunch with Priya, finally";
    note(
      "j-note-lunch",
      today,
      13,
      12,
      lunch,
      [["ent-priya", { new: true, memoryCount: 2 }]],
      { source: "phone", fromDevice: "My phone" },
      {
        feeling: "j-ci-pleased",
        says: { text: "At lunch you finally **saw Priya**, outside the café on Mill Lane." },
      },
    );
    checkin(
      "j-ci-pleased",
      today,
      13,
      12,
      feeling("litUp", "pleased"),
      null,
      { about: "j-note-lunch", fromDevice: "My phone" },
      [],
      { text: "At lunch you were **pleased**." },
    );

    item("j-photo", "image", today, 15, 40, {
      title: "Tomatoes ripening on a windowsill.",
      summary: unread ? null : "Tomatoes ripening on a windowsill.",
      status: "understood",
      source: "phone",
      fromDevice: "My phone",
    });
    data.recs.push({
      t: "note",
      day: today,
      itemId: "j-photo",
      row: "photo",
      mentions: [],
      leftOut: false,
      feeling: null,
      moveTo: null,
    });

    checkin(
      "j-ci-relieved",
      today,
      18,
      10,
      feeling("settled", "relieved", "content"),
      "Gran’s scan came back clear.",
      {},
      [["ent-gran"]],
      { text: "At 6:10 pm you checked in as **relieved and content**.", ids: ["j-ci-relieved"] },
    );
    good("j-good-gate", today, 19, 30, "Sam fixed the gate without being asked.", [["ent-sam"]], {
      text: "Sam fixed the gate without being asked.",
    });
    good("j-good-news", today, 20, 0, "Gran’s news.", [["ent-gran"]]);
    good("j-good-tomatoes", today, 20, 30, "The tomatoes are finally red.");

    entryBlock(
      "j-block-1",
      today,
      13,
      30,
      "text",
      { text: paraOne, title: paraOne.slice(0, 60) },
      [
        ["ent-priya", { new: true, memoryCount: 2 }],
        ["ent-mill-lane"],
        ["ent-climbing-wall", { new: true, memoryCount: 1 }],
        ["ent-sam"],
      ],
      {},
      {
        text: "You and **Priya sat outside the café** on Mill Lane even though it was cold.",
      },
    );
    entryBlock("j-block-voice", today, 17, 0, "audio", {
      title: "Voice note",
      durationMs: 102_000,
      text: unread
        ? null
        : "Priya says the wall by the station has a beginners’ night on Saturday.",
      status: unread ? "reading" : "understood",
      originalPath: null,
      hasOriginal: true,
    });
    entryBlock("j-block-photo", today, 17, 5, "image", {
      title: "Tomatoes ripening on a windowsill.",
      summary: unread ? null : "Tomatoes ripening on a windowsill.",
      status: unread ? "reading" : "understood",
    });
    entryBlock(
      "j-block-2",
      today,
      21,
      14,
      "text",
      { text: paraTwo, title: paraTwo.slice(0, 60) },
      [["ent-gran"], ["ent-allotment"]],
      {
        earlier: [
          {
            itemId: "j-block-2-v1",
            text: "Gran rang at six. Her scan came back fine. She’d been at the allotment all day and sounded tired but pleased with herself.",
            keptAt: at(today, 21, 10),
          },
        ],
        changedAt: at(today, 21, 40),
      },
      { text: "**Gran rang at six** to say her scan had come back clear." },
    );
    item("j-block-2-v1", "text", today, 21, 10, {
      journal: "entry",
      source: "journal",
      text: "Gran rang at six. Her scan came back fine. She’d been at the allotment all day and sounded tired but pleased with herself.",
    });

    data.open.push({
      id: "open-climbing",
      sentence: "Climbing with Priya on Saturday. You said you think you’ll go.",
      subject: "Climbing with Priya, Saturday",
      day: today,
      itemId: "j-block-1",
      due: null,
    });

    if (!unread) {
      const sam = data.mentions.get(
        [...data.mentions.values()].find((m) => m.itemId === "j-block-1" && m.name === "Sam")?.id ??
          "",
      );
      if (sam) {
        const quote = "I think I’ll go with her and Sam on Saturday.";
        const target = data.items.get("j-block-1");
        if (target) {
          data.questions.push({
            id: "sn-sam",
            name: "Sam",
            quote,
            receipt: {
              itemId: "j-block-1",
              kind: "text",
              source: "journal",
              label: "Today’s entry",
              keptAt: target.keptAt,
              durationMs: null,
              span: { type: "quote", text: quote },
              unconfirmed: false,
              fromDevice: null,
            },
            options: [
              { entityId: "ent-sam", name: "Sam", role: "your partner" },
              { entityId: "ent-sam-work", name: "Sam", role: "from work" },
            ],
          });
        }
      }
    }
  }

  // ---- The days before: lighter ----
  const wed = addDays(today, -1);
  note("j-wed-gran", wed, 10, 5, "Gran’s scan is tomorrow at ten. Said I’d ring her after.", [
    ["ent-gran"],
  ]);
  note("j-wed-note", wed, 12, 40, "Posted the Hendersons’ invoice on the way to the school gate.", [
    ["ent-hendersons", {}, "Hendersons"],
    ["ent-school", {}, "school"],
  ]);
  checkin("j-wed-ci", wed, 19, 40, feeling("settled", "content"), "A quiet one.", {}, [], {
    text: "In the evening you checked in as **content**.",
  });
  good("j-wed-good", wed, 20, 5, "Leo read to Gran down the phone.", [["ent-leo"], ["ent-gran"]]);
  entryBlock(
    "j-wed-block",
    wed,
    21,
    0,
    "text",
    { text: "Quiet evening. Leo read two pages out loud without being asked." },
    [["ent-leo"]],
  );
  const tue = addDays(today, -2);
  note(
    "j-tue-note",
    tue,
    8,
    15,
    "Rain all day. Cancelled the allotment, did the paperwork instead.",
    [["ent-allotment"]],
  );

  // ---- Last week, Monday to Sunday ----
  const dow = (day: string) => new Date(`${day}T12:00:00`).getDay();
  const monThis = addDays(today, -((dow(today) + 6) % 7));
  const monLast = addDays(monThis, -7);
  const wk = (i: number) => addDays(monLast, i);

  note(
    "j-w-mon",
    wk(0),
    8,
    0,
    "Rough start. School run in the rain and Leo would not wear his coat.",
    [["ent-leo"], ["ent-sam"]],
  );
  checkin("j-w-mon-ci", wk(0), 8, 10, feeling("woundUp", "stressed"), null);
  item("j-w-tue", "audio", wk(1), 18, 20, {
    title: "Voice note",
    durationMs: 12_000,
    text: "Leo read the whole of his school play part to Gran on the phone.",
    source: "recording",
  });
  data.recs.push({
    t: "note",
    day: wk(1),
    itemId: "j-w-tue",
    row: "voice",
    mentions: found("j-w-tue", "Leo read the whole of his school play part to Gran on the phone.", [
      ["ent-leo"],
      ["ent-gran"],
    ]),
    leftOut: false,
    feeling: null,
    moveTo: null,
  });
  checkin("j-w-tue-ci1", wk(1), 7, 30, feeling("low", "tired"), null);
  checkin("j-w-tue-ci2", wk(1), 18, 0, feeling("litUp", "proud"), null);
  entryBlock(
    "j-w-wed",
    wk(2),
    21,
    0,
    "text",
    { text: "Finished the Hendersons’ kitchen. They loved it. Sam made tea." },
    [["ent-hendersons", { new: true }, "Hendersons"], ["ent-sam"], ["ent-leo"]],
  );
  checkin("j-w-wed-ci", wk(2), 21, 30, feeling("settled", "content"), null);
  note("j-w-thu", wk(3), 17, 45, "Gran came for tea. Leo showed her his science book.", [
    ["ent-gran"],
    ["ent-leo"],
  ]);
  checkin("j-w-thu-ci", wk(3), 8, 0, feeling("woundUp", "stressed"), null);
  note("j-w-fri", wk(4), 16, 30, "Sam took Leo swimming, I had the house to myself.", [
    ["ent-sam"],
    ["ent-leo"],
  ]);
  checkin("j-w-fri-ci", wk(4), 19, 45, feeling("low", "tired"), null);
  good(
    "j-w-sat",
    wk(5),
    16,
    0,
    "First frost on the allotment. Dug the last of the leeks with Gran.",
    [["ent-allotment"], ["ent-gran"], ["ent-leo"]],
  );
  checkin("j-w-sat-ci", wk(5), 17, 0, feeling("settled", "content"), null);
  entryBlock(
    "j-w-sun",
    wk(6),
    20,
    0,
    "text",
    {
      text: "Decided to take Fridays off in December. Told Gran at the allotment, she said about time.",
    },
    [["ent-leo"], ["ent-gran"], ["ent-allotment"]],
  );
  checkin("j-w-sun-ci", wk(6), 8, 0, feeling("low", "tired"), null);
  data.highlights.set(wk(1), {
    text: "Leo read the whole of his school play part to Gran on the phone.",
    quoted: true,
    itemId: "j-w-tue",
  });
  data.highlights.set(wk(2), {
    text: "Finished the Hendersons’ kitchen. They loved it.",
    quoted: true,
    itemId: "j-w-wed",
  });
  data.highlights.set(wk(5), {
    text: "First frost on the allotment. Dug the last of the leeks with Gran.",
    quoted: true,
    itemId: "j-w-sat",
  });
  data.highlights.set(wk(6), {
    text: "You decided to take **Fridays off in December**.",
    quoted: false,
    itemId: "j-w-sun",
  });
  data.open.push({
    id: "open-eye-test",
    sentence: "Book Gran’s eye test. You said you would, on Monday.",
    subject: "Book Gran’s eye test",
    day: wk(0),
    itemId: "j-w-mon",
    due: null,
  });

  // ---- The week before that, for the month's counts ----
  const wk2 = (i: number) => addDays(monLast, -7 + i);
  const early: Array<[number, string, number]> = [
    [0, "content", 20],
    [2, "tired", 8],
    [3, "worried", 9],
    [5, "pleased", 10],
    [6, "calm", 19],
  ];
  for (const [i, word, hour] of early) {
    const day = wk2(i);
    if (day.slice(0, 7) !== today.slice(0, 7)) continue;
    const part = PART_BY_WORD[word] ?? "settled";
    checkin(`j-e-${i}`, day, hour, 0, feeling(part, word), null);
  }

  // ---- Last year, for the year's recap ----
  const yearBefore = String(Number(today.slice(0, 4)) - 1);
  const months: Array<[string, string, string]> = [
    ["01", "Sam’s first week at the new job.", "ent-sam"],
    ["02", "Snow day. Leo built a snowman taller than him.", "ent-leo"],
    ["03", "Gran’s 80th. Everyone came.", "ent-gran"],
    ["04", "Started the allotment with Gran.", "ent-allotment"],
    ["05", "Finally said yes to the Hendersons’ kitchen.", "ent-hendersons"],
    ["06", "The week in Cornwall. Best swim of my life.", "ent-leo"],
  ];
  months.forEach(([mm, text, who], index) => {
    const day = `${yearBefore}-${mm}-12`;
    const id = `j-y-${mm}`;
    note(id, day, 19, 0, text, [[who]]);
    data.highlights.set(day, { text, quoted: index !== 3, itemId: id });
  });
  const archive: Array<[string, number]> = [
    ["content", 14],
    ["tired", 12],
    ["proud", 7],
    ["worried", 6],
  ];
  let seq = 0;
  for (const [word, count] of archive) {
    for (let k = 0; k < count; k += 1) {
      seq += 1;
      const monthIndex = (seq * 5) % 12;
      const dayOfMonth = 2 + ((seq * 7) % 26);
      const day = `${yearBefore}-${String(monthIndex + 1).padStart(2, "0")}-${String(dayOfMonth).padStart(2, "0")}`;
      const part = PART_BY_WORD[word] ?? "settled";
      const f = feeling(part, word);
      f.point = {
        pleasant: f.point.pleasant + ((seq % 5) - 2) * 0.06,
        energy: f.point.energy + ((seq % 3) - 1) * 0.08,
      };
      checkin(`j-y-ci-${seq}`, day, 8 + (seq % 11), 0, f, null);
    }
  }

  return data;
}
