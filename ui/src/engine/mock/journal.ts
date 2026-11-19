import { addDays } from "../../copy/format";
import { WORDS } from "../../journal/feelings";
import type {
  CheckIn,
  CheckInInput,
  DayCount,
  DayPart,
  DayReflection,
  EngineStatus,
  EntryBlock,
  Feeling,
  FeelingPart,
  Glance,
  GlanceKind,
  GoodThing,
  ItemView,
  JournalAnswer,
  JournalDay,
  Kept,
  KeptCount,
  MemoryView,
  Mention,
  NameRef,
  Period,
  PeriodKind,
  PeriodSummary,
  QuestionChoice,
  Recap,
  Receipt,
  RefileTarget,
  ReflectionLine,
  ReflectionView,
  RowKind,
  SameNameAnswer,
  SameNameAnswered,
  Source,
  TimelineRow,
  Together,
  WordCount,
  WrittenWhen,
} from "../bindings";
import {
  type JournalData,
  type JournalMode,
  journalDayOf,
  type OpenThing,
  type Rec,
  type Says,
  seedJournal,
} from "./seed/journal";

export interface JournalHost {
  now(): Date;
  /** An item kept elsewhere in the app (a photo or voice note kept for the entry). */
  lookup(id: string): ItemView | undefined;
  /** Tells the app an item changed, so what shows it is read again. */
  changed(item: ItemView): void;
}

export interface JournalOptions {
  mode?: JournalMode;
  /** How long "Reflect" takes. Defaults to a short pause; tests pass 0. */
  delayMs?: number;
}

const clone = <T>(value: T): T => structuredClone(value);

const MODES: ReadonlySet<string> = new Set([
  "day",
  "first",
  "empty",
  "partial",
  "notready",
  "glance",
  "writing",
]);

/** `?mockJournal=<state>` before the `#`. */
export function journalModeFromAddress(): JournalMode | undefined {
  if (typeof location === "undefined") return undefined;
  const found = new URLSearchParams(location.search).get("mockJournal");
  return found && MODES.has(found) ? (found as JournalMode) : undefined;
}

/** `?mockNow=<RFC 3339>` before the `#` sets the mock clock. */
export function mockNowFromAddress(): string | undefined {
  if (typeof location === "undefined") return undefined;
  const found = new URLSearchParams(location.search).get("mockNow");
  return found && !Number.isNaN(new Date(found).getTime()) ? found : undefined;
}

const pad = (n: number) => String(n).padStart(2, "0");
const weekdayOf = (day: string) => new Date(`${day}T12:00:00`).getDay();
const keyOf = (p: Period) => `${p.kind}:${p.start}`;
const stripSpans = (text: string) => text.replaceAll("**", "");

/** The week, month or year a day falls in; weeks run Monday to Sunday. */
export function periodOf(kind: PeriodKind, day: string): Period {
  const [y, m] = day.split("-").map(Number) as [number, number];
  if (kind === "day") return { kind, start: day, end: day };
  if (kind === "week") {
    const start = addDays(day, -((weekdayOf(day) + 6) % 7));
    return { kind, start, end: addDays(start, 6) };
  }
  if (kind === "month") {
    const last = new Date(y, m, 0).getDate();
    return { kind, start: `${y}-${pad(m)}-01`, end: `${y}-${pad(m)}-${pad(last)}` };
  }
  return { kind, start: `${y}-01-01`, end: `${y}-12-31` };
}

const within = (day: string, p: Period) => day >= p.start && day <= p.end;

const partOfHour = (iso: string): DayPart => {
  const h = new Date(iso).getHours();
  if (h < 4) return "afterMidnight";
  if (h < 12) return "morning";
  if (h < 17) return "afternoon";
  return "evening";
};

/** A thing is "together" with a part of the field after this many days. */
const TOGETHER_DAYS = 3;
const FELT_PARTS: FeelingPart[] = ["woundUp", "litUp", "low", "settled"];

/** Things kept from elsewhere (documents, web pages) are not about the day, so they name no one. */
const isBackground = (rec: Rec) => rec.t === "note" && rec.row === "elsewhere";

const GLANCE_ORDER: GlanceKind[] = ["note", "voice", "photo", "document", "web"];

/** The mock journal: state and every journal method of the engine. */
export class MockJournal {
  private readonly data: JournalData;
  private readonly saved = new Map<string, ReflectionView>();
  /** The first sentence shown on the day for a reflection, by period. */
  private readonly headlines = new Map<string, string>();
  private readonly stopped = new Set<string>();
  private readonly declined = new Set<string>();
  private readonly left = new Set<string>();
  private readonly delayMs: number;
  private counter = 0;
  private writingStopped = false;
  /** Answers to the page's question from the journal, kept on today. */
  private answers: { itemId: string; answer: JournalAnswer }[] = [];

  constructor(
    private readonly host: JournalHost,
    options: JournalOptions = {},
  ) {
    const mode = options.mode ?? journalModeFromAddress() ?? "day";
    this.delayMs = options.delayMs ?? 1200;
    this.data = seedJournal(host.now(), mode);
    this.seedSaved();
  }

  /** Whether the engine is still getting ready. */
  get notReady(): boolean {
    return this.data.mode === "notready";
  }

  get mode(): JournalMode {
    return this.data.mode;
  }

  get today(): string {
    return this.data.today;
  }

  /** What the status says while the mock journal is "getting ready". */
  adjustStatus(status: EngineStatus): EngineStatus {
    return this.notReady ? { ...status, state: "gettingReady" } : status;
  }

  /**
   * The quiet line for the page dated `date`: yesterday's reflection when the night wrote it, or
   * a week, month or year that ended yesterday and has its recap, the larger period first.
   */
  pageLine(date: string): Period | undefined {
    const yesterday = addDays(date, -1);
    for (const kind of ["year", "month", "week"] as const) {
      const p = periodOf(kind, yesterday);
      const view = this.saved.get(keyOf(p));
      if (p.end === yesterday && view?.type === "recap" && !view.recap.soFar) return p;
    }
    const view = this.saved.get(keyOf(periodOf("day", yesterday)));
    if (view?.type === "written" && view.reflection.written === "overnight") {
      return periodOf("day", yesterday);
    }
    return undefined;
  }

  /**
   * Takes back an item the journal holds, with any feeling tapped beside it. False when the
   * journal does not hold it.
   */
  unkeep(id: string): boolean {
    if (!this.data.items.has(id)) return false;
    const gone = new Set([id]);
    for (const rec of this.data.recs) {
      if (rec.t === "checkin" && rec.checkin.about === id) gone.add(rec.checkin.id);
    }
    this.data.recs = this.data.recs.filter((rec) => !gone.has(this.recItemId(rec)));
    for (const item of gone) this.data.items.delete(item);
    return true;
  }

  /** Deletes one item the journal holds, and nothing beside it. False when it does not hold it. */
  delete(id: string): boolean {
    if (!this.data.items.has(id)) return false;
    this.data.recs = this.data.recs.filter((rec) => this.recItemId(rec) !== id);
    this.answers = this.answers.filter((a) => a.itemId !== id);
    this.data.items.delete(id);
    return true;
  }

  /** Keeps a choice on the page's question as an answer on today, and gives the item's id. */
  keepAnswer(choice: QuestionChoice, about: string, sentence: string): string {
    const id = this.nextId("j-answer");
    const view = this.newItem(id, "text", this.host.now().toISOString(), {
      title: sentence,
      status: "understood",
      text: "",
    });
    view.journal = "answer";
    this.answers.push({ itemId: id, answer: { choice, line: null, about, sentence } });
    return id;
  }

  /** Adds a line to a kept answer; the line is read like an entry. */
  setAnswerLine(itemId: string, line: string): void {
    const found = this.answers.find((a) => a.itemId === itemId);
    const view = this.data.items.get(itemId);
    if (!found || !view) return;
    found.answer.line = line;
    view.text = line;
    view.title = line.slice(0, 60);
    view.status = "kept";
  }

  /** The check-in kept as item `id`, for its own page. */
  checkinOf(id: string): CheckIn | undefined {
    const rec = this.data.recs.find((r) => r.t === "checkin" && r.checkin.id === id);
    return rec?.t === "checkin" ? clone(rec.checkin) : undefined;
  }

  /** Lines of the day's reflection and recaps that cite any of `ids` (what a deletion takes out). */
  citedLines(ids: string[]): { lines: number; recap: number } {
    const gone = new Set(ids);
    const lines = this.data.recs.filter((rec) => {
      if (this.left.has(this.recItemId(rec))) return false;
      const says = this.sentenceOf(rec);
      if (!says) return false;
      return gone.has(this.recItemId(rec)) || (says.ids ?? []).some((id) => gone.has(id));
    }).length;
    return { lines, recap: lines > 1 ? Math.floor(lines / 2) : 0 };
  }

  /** Items this journal holds, for receipts and item pages. */
  findItem(id: string): ItemView | undefined {
    const found = this.data.items.get(id);
    return found ? clone(found) : undefined;
  }

  /** The open "Which Sam is this?" questions, for Needs you. */
  openQuestions() {
    return clone(this.data.questions);
  }

  // Seeds

  private seedSaved(): void {
    const { today } = this.data;
    const hasToday = this.recsOf(today).length > 0;
    const unwritten = this.data.mode === "notready" || this.data.mode === "glance";
    if (hasToday && !unwritten) {
      const p = periodOf("day", today);
      const view = this.composeDay(p, "onDemand", this.at(today, 15, 15), 6);
      this.saved.set(keyOf(p), view);
      this.headlines.set(keyOf(p), "A slow start, then lunch with Priya at last.");
    }
    const wed = addDays(today, -1);
    if (this.recsOf(wed).length > 0) {
      const p = periodOf("day", wed);
      this.saved.set(keyOf(p), this.composeDay(p, "overnight", this.at(today, 2, 10)));
    }
    const weekNow = periodOf("week", today);
    const week = periodOf("week", addDays(weekNow.start, -7));
    this.saved.set(
      keyOf(week),
      this.composeRecap(week, "overnight", this.at(weekNow.start, 1, 30)),
    );
    const month = periodOf("month", today);
    this.saved.set(keyOf(month), this.composeRecap(month, "onDemand", this.at(today, 21, 40)));
    const year = periodOf("year", `${Number(today.slice(0, 4)) - 1}-06-01`);
    this.saved.set(keyOf(year), this.composeRecap(year, "overnight", this.at(`${year.end}`, 2, 0)));
  }

  private at(day: string, hh: number, mm: number): string {
    const [y, m, d] = day.split("-").map(Number) as [number, number, number];
    return new Date(y, m - 1, d, hh, mm).toISOString();
  }

  private nextId(prefix: string): string {
    this.counter += 1;
    return `${prefix}-${this.counter}`;
  }

  // Records

  private recsOf(day: string): Rec[] {
    return this.data.recs.filter((r) => r.day === day);
  }

  private item(id: string): ItemView {
    const found = this.data.items.get(id) ?? this.host.lookup(id);
    if (!found) throw new Error(`No item ${id}`);
    return found;
  }

  private recItemId(rec: Rec): string {
    switch (rec.t) {
      case "block":
        return rec.block.item.id;
      case "note":
        return rec.itemId;
      case "checkin":
        return rec.checkin.id;
      case "good":
        return rec.good.id;
    }
  }

  private receipt(itemId: string, span: Receipt["span"] = null): Receipt {
    const it = this.item(itemId);
    return {
      itemId,
      kind: it.kind,
      source: it.source,
      label: it.journal === "entry" && it.kind === "text" ? "Today’s entry" : it.title,
      keptAt: it.keptAt,
      durationMs: it.durationMs,
      span,
      unconfirmed: false,
      fromDevice: it.fromDevice,
    };
  }

  /** Every item in a record, newest version first for a block. */
  private checkinsOf(day: string): CheckIn[] {
    return this.recsOf(day)
      .flatMap((r) => (r.t === "checkin" ? [r.checkin] : []))
      .sort((a, b) => a.keptAt.localeCompare(b.keptAt));
  }

  private goodOf(day: string): GoodThing[] {
    return this.recsOf(day)
      .flatMap((r) => (r.t === "good" ? [r.good] : []))
      .sort((a, b) => a.keptAt.localeCompare(b.keptAt));
  }

  private blocksOf(day: string): EntryBlock[] {
    return this.recsOf(day)
      .flatMap((r) => (r.t === "block" ? [r.block] : []))
      .sort((a, b) => a.block - b.block);
  }

  private mentionsOfRec(rec: Rec): Mention[] {
    switch (rec.t) {
      case "block":
        return rec.block.mentions;
      case "note":
        return rec.mentions;
      case "checkin":
        return rec.checkin.mentions;
      case "good":
        return rec.good.mentions;
    }
  }

  private namesOf(recs: Rec[]): NameRef[] {
    const out = new Map<string, NameRef>();
    const ordered = recs
      .filter((r) => !isBackground(r))
      .sort((a, b) =>
        this.item(this.recItemId(a)).keptAt.localeCompare(this.item(this.recItemId(b)).keptAt),
      );
    for (const rec of ordered) {
      for (const m of this.mentionsOfRec(rec)) {
        if (m.state === "removed") continue;
        const known = this.data.entities.get(m.entityId);
        const found = out.get(m.entityId);
        if (found) {
          found.new ||= m.new;
          if (!found.itemIds.includes(m.itemId)) found.itemIds.push(m.itemId);
        } else {
          out.set(m.entityId, {
            entityId: m.entityId,
            name: m.name,
            kind: m.kind,
            role: m.role ?? known?.role ?? null,
            new: m.new,
            itemIds: [m.itemId],
          });
        }
      }
    }
    return [...out.values()];
  }

  private sameNameOf(day: string): JournalDay["sameName"] {
    const ids = new Set(this.recsOf(day).map((r) => this.recItemId(r)));
    return clone(this.data.questions.filter((q) => ids.has(q.receipt.itemId)));
  }

  // The day

  async getJournalDay(day?: string): Promise<JournalDay> {
    const d = day ?? this.data.today;
    const recs = this.recsOf(d);
    const checkins = this.checkinsOf(d);
    const blocks = this.blocksOf(d);
    const rows: TimelineRow[] = [];
    for (const rec of recs) {
      if (rec.t === "block") continue;
      if (rec.t === "checkin" && rec.checkin.about !== null) continue;
      const base = {
        part: "morning" as DayPart,
        checkin: null,
        good: null,
        feeling: null,
        mentions: [] as Mention[],
        leftOut: false,
        moveTo: null as string | null,
      };
      let row: TimelineRow;
      if (rec.t === "note") {
        const kind: RowKind =
          rec.row === "voice"
            ? "voice"
            : rec.row === "photo"
              ? "photo"
              : rec.row === "elsewhere"
                ? "elsewhere"
                : "note";
        row = {
          ...base,
          kind,
          item: this.item(rec.itemId),
          feeling: checkins.find((c) => c.id === rec.feeling) ?? null,
          mentions: rec.mentions,
          leftOut: rec.leftOut,
          moveTo: rec.moveTo,
        };
      } else if (rec.t === "checkin") {
        row = {
          ...base,
          kind: "checkin",
          item: this.item(rec.checkin.id),
          checkin: rec.checkin,
          mentions: rec.checkin.mentions,
          leftOut: rec.checkin.leftOut,
        };
      } else {
        row = {
          ...base,
          kind: "good",
          item: this.item(rec.good.id),
          good: rec.good,
          mentions: rec.good.mentions,
          leftOut: rec.good.leftOut,
        };
      }
      row.part = partOfHour(row.item.keptAt);
      rows.push(row);
    }
    const last = blocks.at(-1);
    const savedAt = blocks.reduce<string | null>(
      (latest, b) => (latest === null || b.item.keptAt > latest ? b.item.keptAt : latest),
      null,
    );
    if (last && savedAt) {
      const marker = blocks.find((b) => b.item.keptAt === savedAt) ?? last;
      rows.push({
        kind: "entry",
        part: partOfHour(savedAt),
        item: marker.item,
        checkin: null,
        good: null,
        feeling: null,
        mentions: [],
        leftOut: false,
        moveTo: null,
      });
    }
    if (d === this.data.today) {
      for (const { itemId, answer } of this.answers) {
        const item = this.item(itemId);
        rows.push({
          kind: "answer",
          part: partOfHour(item.keptAt),
          item,
          checkin: null,
          good: null,
          feeling: null,
          mentions: [],
          leftOut: false,
          moveTo: null,
          answer,
        });
      }
    }
    rows.sort((a, b) => a.item.keptAt.localeCompare(b.item.keptAt));
    const allRead = blocks.every((b) => b.item.status === "understood");
    const readAt =
      blocks.length > 0 && allRead && savedAt
        ? new Date(new Date(savedAt).getTime() + 10 * 60_000).toISOString()
        : null;
    const reflection = this.saved.get(keyOf(periodOf("day", d)));
    const summary =
      reflection && (reflection.type === "written" || reflection.type === "recap")
        ? this.summaryOf(reflection, keyOf(periodOf("day", d)))
        : null;
    const reading = recs.filter((r) => {
      const status = this.item(this.recItemId(r)).status;
      return status === "kept" || status === "reading";
    }).length;
    return clone({
      day: d,
      today: d === this.data.today,
      firstUse: this.data.firstUse,
      entry: blocks,
      entrySavedAt: savedAt,
      entryReadAt: readAt,
      rows,
      checkins,
      good: this.goodOf(d).slice(0, 3),
      people: this.namesOf(recs),
      reflection: summary,
      reading,
      sameName: this.sameNameOf(d),
    });
  }

  private summaryOf(view: ReflectionView, key: string): JournalDay["reflection"] {
    if (view.type !== "written") return null;
    const r = view.reflection;
    const first = r.theDay[0] ? stripSpans(r.theDay[0].text) : "";
    return {
      firstSentence: this.headlines.get(key) ?? first,
      writtenAt: r.writtenAt,
      fromCount: r.fromCount,
    };
  }

  async listJournalDays(): Promise<string[]> {
    return [...new Set(this.data.recs.map((r) => r.day))].sort();
  }

  async listPeriods(kind: PeriodKind): Promise<PeriodSummary[]> {
    const seen = new Map<string, Period>();
    for (const rec of this.data.recs) {
      const p = periodOf(kind, rec.day);
      seen.set(keyOf(p), p);
    }
    return [...seen.values()]
      .sort((a, b) => b.start.localeCompare(a.start))
      .map((period) => ({ period, written: this.saved.has(keyOf(period)) }));
  }

  async dismissJournalIntro(): Promise<void> {
    this.data.firstUse = false;
  }

  // Writing

  async saveEntryText(
    day: string,
    itemId: string | null,
    block: number,
    text: string,
  ): Promise<EntryBlock | null> {
    const clean = text.trim();
    const now = this.host.now().toISOString();
    if (itemId === null) {
      if (clean === "") return null;
      for (const b of this.blocksOf(day)) if (b.block >= block) b.block += 1;
      const id = this.nextId("j-new");
      const view = this.newItem(id, "text", now, { text, title: clean.slice(0, 60) });
      view.journal = "entry";
      view.source = "journal";
      const made: EntryBlock = {
        item: view,
        block,
        mentions: [],
        earlier: [],
        changedAt: null,
        alt: null,
      };
      this.data.recs.push({ t: "block", day, block: made });
      this.host.changed(clone(view));
      return clone(made);
    }
    const rec = this.data.recs.find((r) => r.t === "block" && r.block.item.id === itemId);
    if (rec?.t !== "block") throw new Error(`No entry block ${itemId}`);
    const current = rec.block;
    const unread = current.item.status === "kept" || current.item.status === "reading";
    if (unread) {
      if (clean === "") {
        this.data.recs.splice(this.data.recs.indexOf(rec), 1);
        this.data.items.delete(itemId);
        for (const b of this.blocksOf(day)) if (b.block > current.block) b.block -= 1;
        this.host.changed(clone(current.item));
        return null;
      }
      current.item.text = text;
      current.item.title = clean.slice(0, 60);
      current.item.keptAt = now;
      this.host.changed(clone(current.item));
      return clone(current);
    }
    const id = this.nextId("j-new");
    const earlier = {
      itemId: current.item.id,
      text: current.item.text,
      keptAt: current.item.keptAt,
    };
    const view = this.newItem(id, "text", now, { text, title: clean.slice(0, 60) });
    view.journal = "entry";
    view.source = "journal";
    rec.block = {
      ...current,
      item: view,
      earlier: [earlier, ...current.earlier],
      changedAt: now,
      mentions: [],
    };
    this.host.changed(clone(view));
    return clone(rec.block);
  }

  private newItem(
    id: string,
    kind: ItemView["kind"],
    keptAt: string,
    extra: Partial<ItemView> = {},
  ): ItemView {
    const view: ItemView = {
      id,
      kind,
      title: "Note",
      source: "journal",
      trust: "trusted",
      keptAt,
      status: "kept",
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
      ...extra,
    };
    this.data.items.set(id, view);
    return view;
  }

  async placeInEntry(itemId: string, day: string, block: number): Promise<EntryBlock> {
    const source = this.host.lookup(itemId) ?? this.data.items.get(itemId);
    if (!source) throw new Error(`No item ${itemId}`);
    for (const b of this.blocksOf(day)) if (b.block >= block) b.block += 1;
    const view = { ...clone(source), journal: "entry" as const, source: "journal" as Source };
    this.data.items.set(itemId, view);
    const made: EntryBlock = {
      item: view,
      block,
      mentions: [],
      earlier: [],
      changedAt: null,
      alt: null,
    };
    this.data.recs.push({ t: "block", day, block: made });
    this.host.changed(clone(view));
    return clone(made);
  }

  async describePhoto(itemId: string, text: string): Promise<void> {
    const rec = this.data.recs.find((r) => r.t === "block" && r.block.item.id === itemId);
    if (rec?.t !== "block") throw new Error(`No photo ${itemId}`);
    rec.block.alt = text;
  }

  async readEntryNow(day: string): Promise<void> {
    for (const b of this.blocksOf(day)) {
      if (b.item.status === "kept" || b.item.status === "reading") {
        b.item.status = "understood";
        this.host.changed(clone(b.item));
      }
    }
  }

  async keepNote(
    text: string,
    source: Source,
    feeling: Feeling | null,
    good: boolean,
  ): Promise<Kept> {
    const now = this.host.now();
    const iso = now.toISOString();
    const day = journalDayOf(now);
    const clean = text.trim();
    const id = this.nextId("j-new");
    const view = this.newItem(id, "text", iso, {
      text: clean,
      title: clean.slice(0, 60),
      source,
      journal: good ? "good" : undefined,
    });
    let tapped: string | null = null;
    if (feeling) {
      const ciId = this.nextId("j-new");
      this.newItem(ciId, "checkin", iso, { title: "Check-in", source });
      this.data.recs.push({
        t: "checkin",
        day,
        checkin: this.makeCheckIn(ciId, day, iso, feeling, null, null, id),
      });
      tapped = ciId;
    }
    if (good) {
      this.data.recs.push({
        t: "good",
        day,
        good: {
          id,
          text: clean,
          day,
          keptAt: iso,
          photo: null,
          fromDevice: null,
          mentions: [],
          leftOut: false,
        },
      });
    } else {
      this.data.recs.push({
        t: "note",
        day,
        itemId: id,
        row: "note",
        mentions: [],
        leftOut: false,
        feeling: tapped,
        moveTo: null,
      });
    }
    this.host.changed(clone(view));
    return { type: "new", item: clone(view) };
  }

  private makeCheckIn(
    id: string,
    day: string,
    keptAt: string,
    feeling: Feeling,
    line: string | null,
    urge: string | null,
    about: string | null,
  ): CheckIn {
    return {
      id,
      keptAt,
      day,
      feeling,
      line,
      urge,
      about,
      tried: [],
      fromDevice: null,
      mentions: [],
      leftOut: false,
    };
  }

  async topFeelingWords(part: FeelingPart): Promise<string[]> {
    const used = new Map<string, number>();
    for (const rec of this.data.recs) {
      if (rec.t !== "checkin" || rec.checkin.feeling.part !== part) continue;
      for (const w of rec.checkin.feeling.words) used.set(w, (used.get(w) ?? 0) + 1);
    }
    const ranked = [...used.entries()].sort((a, b) => b[1] - a[1]).map(([w]) => w);
    const first = WORDS[part].slice(0, 12).map((w) => w.word);
    return [...ranked, ...first.filter((w) => !ranked.includes(w))].slice(0, 4);
  }

  async keepCheckIn(input: CheckInInput): Promise<CheckIn> {
    const now = this.host.now();
    const iso = now.toISOString();
    let day = journalDayOf(now);
    if (input.replaces) {
      const old = this.data.recs.find((r) => r.t === "checkin" && r.checkin.id === input.replaces);
      if (old && old.t === "checkin") {
        day = old.day;
        this.data.recs.splice(this.data.recs.indexOf(old), 1);
      }
    }
    const id = this.nextId("j-new");
    const view = this.newItem(id, "checkin", iso, {
      title: "Check-in",
      text: input.line,
      source: "journal",
    });
    const made = this.makeCheckIn(id, day, iso, input.feeling, input.line, input.urge, null);
    this.data.recs.push({ t: "checkin", day, checkin: made });
    this.host.changed(clone(view));
    return clone(made);
  }

  async markTried(checkinId: string, idea: string): Promise<void> {
    const rec = this.data.recs.find((r) => r.t === "checkin" && r.checkin.id === checkinId);
    if (rec?.t !== "checkin") throw new Error(`No check-in ${checkinId}`);
    if (!rec.checkin.tried.includes(idea)) rec.checkin.tried.push(idea);
  }

  async keepGoodThing(day: string, text: string, photoItemId: string | null): Promise<GoodThing> {
    if (this.goodOf(day).length >= 3) throw new Error("Three good things a day is the most.");
    const iso = this.host.now().toISOString();
    const id = this.nextId("j-new");
    const view = this.newItem(id, "text", iso, {
      text,
      title: text.slice(0, 60),
      journal: "good",
    });
    const photo = photoItemId
      ? (this.data.items.get(photoItemId) ?? this.host.lookup(photoItemId))
      : null;
    const made: GoodThing = {
      id,
      text,
      day,
      keptAt: iso,
      photo: photo ? clone(photo) : null,
      fromDevice: null,
      mentions: [],
      leftOut: false,
    };
    this.data.recs.push({ t: "good", day, good: made });
    this.host.changed(clone(view));
    return clone(made);
  }

  async setLeftOut(itemId: string, leftOut: boolean): Promise<void> {
    if (leftOut) this.left.add(itemId);
    else this.left.delete(itemId);
    for (const rec of this.data.recs) {
      if (this.recItemId(rec) !== itemId) continue;
      if (rec.t === "note") rec.leftOut = leftOut;
      else if (rec.t === "checkin") rec.checkin.leftOut = leftOut;
      else if (rec.t === "good") rec.good.leftOut = leftOut;
    }
    this.host.changed(clone(this.item(itemId)));
  }

  async moveToDay(itemId: string, day: string): Promise<void> {
    const rec = this.data.recs.find((r) => this.recItemId(r) === itemId);
    if (!rec) throw new Error(`No item ${itemId}`);
    const view = this.item(itemId);
    const at = new Date(view.keptAt);
    view.keptAt = this.at(day, at.getHours(), at.getMinutes());
    rec.day = day;
    if (rec.t === "note") rec.moveTo = null;
    if (rec.t === "checkin") rec.checkin.day = day;
    if (rec.t === "good") rec.good.day = day;
    for (const other of this.data.recs) {
      if (other.t === "checkin" && other.checkin.about === itemId) {
        other.day = day;
        other.checkin.day = day;
      }
    }
    this.host.changed(clone(view));
  }

  // People and places

  private mention(id: string): Mention {
    const found = this.data.mentions.get(id);
    if (!found) throw new Error(`No mention ${id}`);
    return found;
  }

  async confirmMention(mentionId: string, role: string | null): Promise<Mention> {
    const m = this.mention(mentionId);
    m.state = "confirmed";
    if (role?.trim()) m.role = role.trim();
    return clone(m);
  }

  async removeMention(mentionId: string): Promise<Mention> {
    const m = this.mention(mentionId);
    m.state = "removed";
    return clone(m);
  }

  async restoreMention(mentionId: string): Promise<Mention> {
    const m = this.mention(mentionId);
    m.state = "found";
    return clone(m);
  }

  async refileMention(mentionId: string, target: RefileTarget): Promise<Mention> {
    const m = this.mention(mentionId);
    if (target.type === "existing") {
      const known = this.data.entities.get(target.entityId);
      if (!known) throw new Error(`No entity ${target.entityId}`);
      Object.assign(m, {
        entityId: known.id,
        name: known.name,
        kind: known.kind,
        role: known.role,
      });
      m.new = false;
    } else {
      const id = this.nextId("ent-new");
      this.data.entities.set(id, { id, name: target.name, kind: target.kind, role: null });
      Object.assign(m, { entityId: id, name: target.name, kind: target.kind, role: null });
      m.new = true;
    }
    m.state = "confirmed";
    return clone(m);
  }

  async answerSameName(questionId: string, answer: SameNameAnswer): Promise<SameNameAnswered> {
    const q = this.data.questions.find((x) => x.id === questionId);
    if (!q) throw new Error(`No question ${questionId}`);
    const m = [...this.data.mentions.values()].find(
      (x) => x.itemId === q.receipt.itemId && x.name === q.name && x.state === "found",
    );
    this.data.questions = this.data.questions.filter((x) => x.id !== questionId);
    if (answer.type === "leave") return { name: q.name, role: null };
    if (answer.type === "new") {
      const id = this.nextId("ent-new");
      this.data.entities.set(id, { id, name: q.name, kind: "person", role: null });
      if (m) Object.assign(m, { entityId: id, role: null, state: "confirmed", new: true });
      return { name: q.name, role: null };
    }
    const known = this.data.entities.get(answer.entityId);
    if (!known) throw new Error(`No entity ${answer.entityId}`);
    if (m) Object.assign(m, { entityId: known.id, role: known.role, state: "confirmed" });
    return { name: known.name, role: known.role };
  }

  async entityJournalDays(entityId: string): Promise<string[]> {
    const days = new Set<string>();
    for (const rec of this.data.recs) {
      if (this.mentionsOfRec(rec).some((m) => m.entityId === entityId && m.state !== "removed")) {
        days.add(rec.day);
      }
    }
    return [...days].sort().reverse();
  }

  // Looking back

  private recsIn(p: Period): Rec[] {
    return this.data.recs.filter((r) => within(r.day, p));
  }

  private glance(p: Period): Glance {
    const recs = this.recsIn(p).filter((r) => !this.left.has(this.recItemId(r)));
    const counts = new Map<GlanceKind, string[]>();
    const add = (kind: GlanceKind, id: string) =>
      counts.set(kind, [...(counts.get(kind) ?? []), id]);
    for (const rec of recs) {
      if (rec.t === "checkin") continue;
      const it = this.item(this.recItemId(rec));
      if (it.kind === "audio") add("voice", it.id);
      else if (it.kind === "image") add("photo", it.id);
      else if (it.kind === "document") add("document", it.id);
      else if (it.kind === "web") add("web", it.id);
      else add("note", it.id);
    }
    const kept: KeptCount[] = GLANCE_ORDER.filter((k) => counts.has(k))
      .map((kind) => ({ kind, itemIds: counts.get(kind) ?? [] }))
      .sort((a, b) => b.itemIds.length - a.itemIds.length);
    const names = this.namesOf(recs);
    const firstBlock = recs
      .flatMap((r) => (r.t === "block" && r.block.item.kind === "text" ? [r.block] : []))
      .sort((a, b) => a.block - b.block)[0];
    const lines = firstBlock?.item.text?.split(/(?<=[.!?])\s+/).slice(0, 2) ?? [];
    return clone({
      period: p,
      kept,
      felt: recs
        .flatMap((r) => (r.t === "checkin" ? [r.checkin] : []))
        .sort((a, b) => a.keptAt.localeCompare(b.keptAt)),
      good: recs.flatMap((r) => (r.t === "good" ? [r.good] : [])).slice(0, 3),
      with: [
        ...names.filter((n) => n.kind === "person"),
        ...names.filter((n) => n.kind !== "person"),
      ],
      entryLines: lines,
      entryItemId: firstBlock?.item.id ?? null,
    });
  }

  private openIn(p: Period): MemoryView[] {
    return this.data.open.filter((o) => within(o.day, p)).map((o) => this.memoryOf(o));
  }

  private memoryOf(o: OpenThing): MemoryView {
    return {
      id: o.id,
      sentence: o.sentence,
      subject: o.subject,
      entityId: null,
      predicate: "plans",
      object: o.subject,
      certainty: "fairlySure",
      status: "active",
      validFrom: null,
      validTo: null,
      due: o.due,
      learnedAt: this.item(o.itemId).keptAt,
      lastUsed: null,
      letGoAt: null,
      keepForever: false,
      private: false,
      suggestPrivate: false,
      was: null,
      movedFrom: null,
      receipts: [this.receipt(o.itemId)],
    };
  }

  private sentenceOf(rec: Rec): Says | null {
    if (rec.says) return rec.says;
    if (rec.t === "checkin") return null;
    const it = this.item(this.recItemId(rec));
    if (rec.t === "note" && rec.row === "elsewhere")
      return { text: `You also kept **${it.title}**.` };
    const text = it.text ?? it.summary;
    if (!text) return null;
    const first = text.split(/(?<=[.!?])\s+/)[0] ?? text;
    if (it.kind === "audio")
      return { text: `In a voice note you said **“${first.slice(0, 80)}”**.` };
    if (it.kind === "image") return { text: `You kept a photo of **${first.slice(0, 80)}**` };
    return { text: `You wrote **“${first.slice(0, 80)}”**` };
  }

  private composeDay(
    p: Period,
    written: WrittenWhen,
    writtenAt: string,
    fromCount?: number,
  ): ReflectionView {
    const day = p.start;
    const recs = this.recsOf(day).filter((r) => !this.left.has(this.recItemId(r)));
    const sorted = [...recs].sort((a, b) =>
      this.item(this.recItemId(a)).keptAt.localeCompare(this.item(this.recItemId(b)).keptAt),
    );
    const theDay: ReflectionLine[] = [];
    for (const rec of sorted) {
      const says = this.sentenceOf(rec);
      if (!says || theDay.length >= 6) continue;
      const spans = says.text.split("**").length > 1 ? (says.text.split("**").length - 1) / 2 : 0;
      const receipts: Receipt[] = [];
      for (let i = 0; i < Math.max(1, spans); i += 1) {
        receipts.push(this.receipt(says.ids?.[i] ?? this.recItemId(rec), says.spans?.[i] ?? null));
      }
      theDay.push({ text: says.text, receipts });
    }
    const reflection: DayReflection = {
      id: `refl-${day}`,
      period: p,
      writtenAt,
      written,
      soFar: day === this.data.today,
      fromCount: fromCount ?? recs.length,
      theDay,
      felt: this.checkinsOf(day).filter((c) => !c.leftOut),
      good: this.goodOf(day).filter((g) => !g.leftOut),
      people: this.namesOf(recs),
      stillOpen: this.openIn(p),
      stillReading: recs.filter((r) =>
        ["kept", "reading"].includes(this.item(this.recItemId(r)).status),
      ).length,
      linesRemoved: 0,
    };
    return { type: "written", reflection: clone(reflection) };
  }

  private composeRecap(p: Period, written: WrittenWhen, writtenAt: string): ReflectionView {
    const recs = this.recsIn(p).filter((r) => !this.left.has(this.recItemId(r)));
    const days = [...new Set(recs.map((r) => r.day))].sort();
    const checkins = recs
      .flatMap((r) => (r.t === "checkin" ? [r.checkin] : []))
      .sort((a, b) => a.keptAt.localeCompare(b.keptAt));
    const wordMap = new Map<string, WordCount>();
    for (const c of checkins) {
      for (const word of c.feeling.words) {
        const found = wordMap.get(word);
        if (found) found.checkinIds.push(c.id);
        else wordMap.set(word, { word, part: c.feeling.part, checkinIds: [c.id] });
      }
    }
    const words = [...wordMap.values()].sort(
      (a, b) => b.checkinIds.length - a.checkinIds.length || a.word.localeCompare(b.word),
    );
    const names = this.namesOf(recs);
    const people: DayCount[] = names
      .map((n) => ({
        with: n,
        days: [
          ...new Set(
            recs
              .filter((r) => !isBackground(r))
              .filter((r) =>
                this.mentionsOfRec(r).some(
                  (m) => m.entityId === n.entityId && m.state !== "removed",
                ),
              )
              .map((r) => r.day),
          ),
        ].sort(),
      }))
      .sort((a, b) => b.days.length - a.days.length || a.with.name.localeCompare(b.with.name));
    const feltOn = new Map<string, CheckIn>();
    for (const c of checkins) {
      const key = `${c.day}|${c.feeling.part}`;
      if (!feltOn.has(key)) feltOn.set(key, c);
    }
    const together: Together[] = [];
    if (p.kind !== "day") {
      for (const person of people) {
        if (person.days.length < TOGETHER_DAYS) continue;
        let best: { part: FeelingPart; hits: CheckIn[] } | null = null;
        for (const part of FELT_PARTS) {
          const hits = person.days.flatMap((d) => feltOn.get(`${d}|${part}`) ?? []);
          if (hits.length >= TOGETHER_DAYS && (!best || hits.length > best.hits.length)) {
            best = { part, hits };
          }
        }
        if (best) {
          together.push({
            count: best.hits.length,
            total: person.days.length,
            with: person.with,
            part: best.part,
            word: null,
            receipts: best.hits.map((c) => this.receipt(c.id)),
          });
        }
      }
      together.sort((a, b) => b.count - a.count || a.with.name.localeCompare(b.with.name));
      together.length = Math.min(together.length, 3);
    }
    const clinicItem = recs
      .filter((r) => !isBackground(r))
      .filter((r) => this.mentionsOfRec(r).some((m) => m.entityId === "ent-gran"))
      .map((r) => this.item(this.recItemId(r)))
      .sort((a, b) => b.keptAt.localeCompare(a.keptAt))[0];
    const highlightsAll = [...this.data.highlights.entries()]
      .filter(([day]) => within(day, p))
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([day, h]) => {
        const ofDay = checkins.filter((c) => c.day === day);
        return {
          day,
          text: h.text,
          quoted: h.quoted,
          receipts: [this.receipt(h.itemId)],
          photo: h.photoId ? clone(this.item(h.photoId)) : null,
          feeling: ofDay.length === 1 ? (ofDay[0]?.feeling ?? null) : null,
        };
      });
    const limit = p.kind === "week" ? 5 : p.kind === "month" ? 8 : 12;
    const goodAll = recs
      .flatMap((r) => (r.t === "good" ? [r.good] : []))
      .sort((a, b) => a.keptAt.localeCompare(b.keptAt));
    const previous = periodOf(p.kind, addDays(p.start, -1));
    const previousCount = this.recsIn(previous).filter((r) => r.t === "checkin").length;
    const recap: Recap = {
      id: `recap-${keyOf(p)}`,
      period: p,
      writtenAt,
      written,
      soFar: this.data.today <= p.end,
      days: days.length,
      highlights: highlightsAll.slice(0, limit),
      checkins,
      words,
      together,
      compare:
        p.kind !== "week" && previousCount > 0
          ? { count: checkins.length, previous: previousCount, previousPeriod: previous }
          : null,
      good: goodAll,
      people,
      newNames: names.filter((n) => n.new),
      tried: checkins.flatMap((c) =>
        c.tried.map((idea) => ({ idea, day: c.day, checkinId: c.id })),
      ),
      changed:
        p.kind === "year" || !clinicItem
          ? []
          : [
              {
                id: "line-changed-clinic",
                kind: "changed",
                text: "Gran’s clinic visit moved to **Thursday**.",
                memoryIds: [],
                receipts: [this.receipt(clinicItem.id)],
                choices: [],
                conflictId: null,
                subject: "Gran’s clinic visit",
                done: false,
                forgotten: false,
                on: null,
                lastUsed: null,
              },
            ],
      stillOpen: p.kind === "year" ? [] : this.openIn(p),
      linesRemoved: 0,
      stillReading: recs.filter((r) =>
        ["kept", "reading"].includes(this.item(this.recItemId(r)).status),
      ).length,
    };
    return { type: "recap", recap: clone(recap) };
  }

  /** Why nothing is written for `p`, in the engine's order, or null when it can be written. */
  private blocked(p: Period): ReflectionView | null {
    const glance = this.glance(p);
    if (this.data.mode === "writing" && !this.writingStopped) return { type: "writing", glance };
    if (glance.kept.length === 0 && glance.felt.length === 0 && glance.good.length === 0) {
      return { type: "nothing", period: p };
    }
    if (p.kind !== "day") return null;
    if (this.data.mode === "notready") return { type: "glance", glance, reason: "notReady" };
    if (this.data.mode === "glance") return { type: "glance", glance, reason: "notOnThisComputer" };
    return null;
  }

  /** A saved reflection with lines that cite anything left out hidden at once. */
  private visible(view: ReflectionView): ReflectionView {
    if (this.left.size === 0) return clone(view);
    const keep = (id: string) => !this.left.has(id);
    const cites = (receipts: Receipt[]) => receipts.every((x) => keep(x.itemId));
    if (view.type === "written") {
      const r = clone(view.reflection);
      r.theDay = r.theDay.filter((line) => cites(line.receipts));
      r.felt = r.felt.filter((c) => keep(c.id));
      r.good = r.good.filter((g) => keep(g.id));
      for (const n of r.people) n.itemIds = n.itemIds.filter(keep);
      r.people = r.people.filter((n) => n.itemIds.length > 0);
      r.stillOpen = r.stillOpen.filter((m) => cites(m.receipts));
      return { type: "written", reflection: r };
    }
    if (view.type === "recap") {
      const r = clone(view.recap);
      r.highlights = r.highlights.filter(
        (h) => cites(h.receipts) && (!h.photo || keep(h.photo.id)),
      );
      r.together = r.together.filter((x) => cites(x.receipts));
      for (const x of r.together) x.with.itemIds = x.with.itemIds.filter(keep);
      r.changed = r.changed.filter((l) => cites(l.receipts));
      r.checkins = r.checkins.filter((c) => keep(c.id));
      for (const w of r.words) w.checkinIds = w.checkinIds.filter(keep);
      r.words = r.words.filter((w) => w.checkinIds.length > 0);
      r.good = r.good.filter((g) => keep(g.id));
      for (const g of r.good) if (g.photo && !keep(g.photo.id)) g.photo = null;
      for (const d of r.people) d.with.itemIds = d.with.itemIds.filter(keep);
      r.people = r.people.filter((d) => d.with.itemIds.length > 0);
      for (const n of r.newNames) n.itemIds = n.itemIds.filter(keep);
      r.newNames = r.newNames.filter((n) => n.itemIds.length > 0);
      r.tried = r.tried.filter((x) => keep(x.checkinId));
      r.stillOpen = r.stillOpen.filter((m) => cites(m.receipts));
      if (r.compare) r.compare.count = r.checkins.length;
      return { type: "recap", recap: r };
    }
    return clone(view);
  }

  async getReflection(period: Period): Promise<ReflectionView> {
    const saved = this.saved.get(keyOf(period));
    const writing = this.data.mode === "writing" && !this.writingStopped;
    if (saved && !writing) return this.visible(saved);
    if (this.declined.has(keyOf(period)) && !writing) {
      const glance = this.glance(period);
      return glance.kept.length === 0 && glance.felt.length === 0 && glance.good.length === 0
        ? { type: "nothing", period }
        : { type: "notYet", glance };
    }
    const gate = this.blocked(period);
    if (gate) return gate;
    return { type: "notYet", glance: this.glance(period) };
  }

  async reflect(period: Period): Promise<ReflectionView> {
    const key = keyOf(period);
    this.stopped.delete(key);
    this.declined.delete(key);
    const gate = this.blocked(period);
    if (gate) return gate;
    if (this.delayMs > 0) await new Promise((resolve) => setTimeout(resolve, this.delayMs));
    if (this.stopped.has(key)) {
      this.stopped.delete(key);
      return this.getReflection(period);
    }
    const now = this.host.now().toISOString();
    const view =
      period.kind === "day"
        ? this.composeDay(period, "onDemand", now)
        : this.composeRecap(period, "onDemand", now);
    this.saved.set(key, view);
    this.headlines.delete(key);
    return this.visible(view);
  }

  async stopReflecting(period: Period): Promise<void> {
    this.stopped.add(keyOf(period));
    if (this.data.mode === "writing") this.writingStopped = true;
  }

  async deleteReflection(period: Period): Promise<void> {
    this.declined.add(keyOf(period));
    this.saved.delete(keyOf(period));
    this.headlines.delete(keyOf(period));
  }
}
