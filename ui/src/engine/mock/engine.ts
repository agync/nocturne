import pkg from "../../../package.json";
import { formatLongDay, localDay } from "../../copy/format";
import type {
  AdvancedView,
  AskDelta,
  AskPlace,
  BackupReport,
  BackupsView,
  CameInRow,
  ChangePreview,
  CheckOutcome,
  DeviceView,
  Edition,
  EngineStatus,
  EntityKind,
  EntityView,
  ExportReport,
  ForgetPreview,
  ForgetReport,
  GuideView,
  HealthSeries,
  ItemFilter,
  ItemKind,
  ItemView,
  Job,
  Kept,
  MemoryFilter,
  MemoryView,
  MergeOutcome,
  ModelChoice,
  ModelsView,
  NeedsYouEntry,
  Notice,
  NoticeCode,
  OnlineForm,
  OnlineSend,
  OnlineSettings,
  OnlineSettingsPatch,
  OnlineView,
  PageLine,
  PageSummary,
  PageView,
  PairingOffer,
  PhoneDeletePreview,
  PhoneDeleteScope,
  PhonePageOptions,
  PhoneSource,
  PhoneSourcesView,
  PrivacyView,
  QuestionChoice,
  ReadingKind,
  ReadingRow,
  Receipt,
  Refiled,
  RefileTarget,
  RemoteConnection,
  RemoteEvent,
  RemoteSettingsPatch,
  RemoteView,
  RenameOutcome,
  RestoreCheck,
  RestoredView,
  SettingsPatch,
  SettingsView,
  SetupView,
  Source,
  SpokenKind,
  SpokenPage,
  Tier,
  WhichResolved,
} from "../bindings";
import type {
  EngineClient,
  EngineEventName,
  EngineEvents,
  EntryAt,
  ForgetItems,
  MoveChoice,
  MoveView,
  RecordingState,
  Unsubscribe,
} from "../client";
import recorded from "./assets/spoken-today.json";
import recordedUrl from "./assets/spoken-today.ogg";
import { MockBackups, type MockBackupsScript } from "./backup";
import { cite, helpView, matchGuide, mockGuide, writtenFor } from "./help";
import { type JournalOptions, MockJournal, mockNowFromAddress } from "./journal";
import { journalQuestionFromAddress, journalQuestionSeed } from "./journalQuestion";
import { ModelsSim } from "./models";
import { OnlineSim } from "./online";
import { answeredBy, onlinePrelude } from "./onlineAsk";
import { PhonesSim } from "./phones";
import { type RemoteOptions, RemoteSim } from "./remote";
import { audioScript } from "./script";
import { buildSeed, DEFAULT_NOW, SAFETY_PREDICATES } from "./seed";
import { PRINT_STRESS_DATE } from "./seed/print";
import { privacySeed } from "./seed/privacy";
import { advancedSeed } from "./seed/setup";

export interface MockOptions {
  /** The fixed clock. Defaults to `VITE_MOCK_NOW`, else 2026-11-13T07:02:00Z. */
  now?: Date | string;
  /** Notices to start with (for tests and the demo). */
  notices?: Notice[];
  /** The smaller mind on offer, when a `tierDropped` notice is among the notices. */
  tierOffer?: Tier;
  /** The tier of a change that did not finish, when a `tierUnfinished` notice is among the notices. */
  unfinishedTier?: Tier;
  /** Whether what was sent to the internet is measured. The real app does not measure it yet. */
  sentMeasured?: boolean;
  /** The first question wakes the voice that reads and says so (a `waking` step with a wait). */
  cold?: boolean;
  /** Answers mark one key phrase per cited memory in bold, as the real app does. On unless false. */
  emphasis?: boolean;
  /** Away from home: where it starts, and whether its scripted steps move by themselves. */
  remote?: RemoteOptions;
  /**
   * Help answers are written from the guide, as on a size of computer that passed the help
   * check. Off by default, as every size is today: the guide's own words show instead.
   * `?mockHelp=written` turns it on.
   */
  helpWritten?: boolean;
  /** The journal's starting state; `?mockJournal=` picks it in the browser. */
  journal?: JournalOptions;
  /** Today's page asks a question from the journal; `?mockQuestion=journal` turns it on. */
  journalQuestion?: boolean;
}

/** `?mockHelp=written` writes help answers in the mock. */
function helpFromAddress(): boolean {
  if (typeof location === "undefined") return false;
  return new URLSearchParams(location.search).get("mockHelp") === "written";
}

/** `?mockAway=<state>`, `?mockStep=<ms>` and `?mockPhone=typed` start Away from home elsewhere than off. */
function remoteFromAddress(): RemoteOptions {
  if (typeof location === "undefined") return {};
  const params = new URLSearchParams(location.search);
  const step = Number(params.get("mockStep"));
  return {
    scenario: params.get("mockAway") ?? undefined,
    stepMs: step > 0 ? step : undefined,
    phone: params.get("mockPhone") === "typed" ? "typed" : undefined,
  };
}

/**
 * `?mockMove=offered|here|room|arriving|checking|restarting|arrived|ranOut|notOpened` opens with a
 * phone's memory moving here.
 */
function moveFromAddress(): MoveView | null {
  if (typeof location === "undefined") return null;
  const name = "Maya’s phone";
  switch (new URLSearchParams(location.search).get("mockMove")) {
    case "offered":
      return { phase: "offered", name, items: 1284, bytes: 1_400_000_000, here: false };
    case "here":
      return { phase: "offered", name, items: 1284, bytes: 1_400_000_000, here: true };
    case "arriving":
      return { phase: "arriving", name, done: 820_000_000, size: 1_400_000_000 };
    case "checking":
      return { phase: "checking", name };
    case "arrived":
      return { phase: "arrived", name, online: false };
    case "room":
      return {
        phase: "offered",
        name,
        items: 1284,
        bytes: 1_400_000_000,
        here: false,
        room: { need: 2_900_000_000, free: 1_200_000_000 },
      };
    case "restarting":
      return { phase: "restarting", name };
    case "ranOut":
      return {
        phase: "failed",
        name,
        why: "noRoom",
        room: { need: 2_900_000_000, free: 0 },
      };
    case "notOpened":
      return { phase: "failed", name, why: "notOpened", room: null };
    default:
      return null;
  }
}

/** How long after keeping something it can still be taken back. */
const UNKEEP_MINUTES = 10;

const clone = <T>(value: T): T => structuredClone(value);

/** Lets go of the sound or picture held for an item that is gone. */
function release(item: ItemView): void {
  if (item.originalPath?.startsWith("blob:") && typeof URL.revokeObjectURL === "function") {
    URL.revokeObjectURL(item.originalPath);
  }
}

/**
 * A recording or photo kept in the browser is held in memory and played from where it is, so the
 * item's page can play it back. Without a browser to hold it, the item keeps its placeholder.
 */
function heldOriginal(bytes: Uint8Array, mime: string): Partial<ItemView> {
  if (typeof URL === "undefined" || typeof URL.createObjectURL !== "function") return {};
  if (!mime.startsWith("audio/") && !mime.startsWith("image/")) return {};
  const copy = new Uint8Array(bytes);
  return { originalPath: URL.createObjectURL(new Blob([copy], { type: mime })) };
}

/** The engine's plain dek, used when the written one could mention something private. */
function templateDek(kept: number, needs: number, letGo: number): string {
  const things = kept === 1 ? "thing" : "things";
  const need = needs === 1 ? "needs" : "need";
  const were = letGo === 1 ? "was" : "were";
  return `Last night Nocturne read ${kept} ${things} you kept. ${needs} ${need} you. ${letGo} ${were} let go.`;
}

/** How long the mock says the first question takes to wake the voice. */
const WAKE_SECONDS = 8;

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * Marks the key phrase of one cited sentence in bold: where the memory's value appears, or, when the
 * sentence words it differently, the stretch that holds all of the value's words.
 */
function emphasise(sentence: string, value: string): string {
  const wrap = (from: number, to: number) =>
    `${sentence.slice(0, from)}**${sentence.slice(from, to)}**${sentence.slice(to)}`;
  const exact = value ? sentence.toLowerCase().indexOf(value.toLowerCase()) : -1;
  if (exact >= 0) return wrap(exact, exact + value.length);
  const spans = value
    .split(/\s+/)
    .filter((w) => w.length > 2)
    .map((w) =>
      new RegExp(`(?<![\\p{L}\\p{N}])${escapeRegExp(w)}[\\p{L}\\p{N}]*`, "iu").exec(sentence),
    );
  if (spans.length === 0 || spans.some((m) => m === null)) return sentence;
  const found = spans as RegExpExecArray[];
  return wrap(
    Math.min(...found.map((m) => m.index)),
    Math.max(...found.map((m) => m.index + m[0].length)),
  );
}

/** Where the mock has no recording, the voice is silence and each section lasts this long. */
const MOCK_SECTION_MS = 4000;

let recordedBlob: Promise<string | null> | null = null;

/**
 * The recorded page as a `blob:` URL, which the player accepts as it does a freshly made one.
 * Null where the file cannot be fetched (a test run), when the page is silent instead.
 */
function recordedPageUrl(): Promise<string | null> {
  recordedBlob ??= (async () => {
    try {
      const response = await fetch(recordedUrl);
      if (!response.ok) return null;
      const blob = await response.blob();
      return typeof URL.createObjectURL === "function" ? URL.createObjectURL(blob) : null;
    } catch {
      return null;
    }
  })();
  return recordedBlob;
}

/** A silent 8 kHz mono 16-bit WAV of `ms` milliseconds, as a `blob:` URL (the CSP allows it). */
function mockSpokenUrl(ms: number): string {
  const rate = 8000;
  const bytes = Math.floor((rate * ms) / 1000) * 2;
  const wav = new ArrayBuffer(44 + bytes);
  const view = new DataView(wav);
  const text = (at: number, value: string) => {
    for (let i = 0; i < value.length; i += 1) view.setUint8(at + i, value.charCodeAt(i));
  };
  text(0, "RIFF");
  view.setUint32(4, 36 + bytes, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, bytes, true);
  if (typeof URL.createObjectURL !== "function") return `mock-audio/silence-${ms}.wav`;
  return URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
}

const STOP_WORDS = new Set([
  "what",
  "when",
  "where",
  "who",
  "whom",
  "which",
  "why",
  "how",
  "the",
  "and",
  "for",
  "are",
  "was",
  "were",
  "did",
  "does",
  "has",
  "have",
  "had",
  "about",
  "with",
  "from",
  "that",
  "this",
  "still",
  "my",
  "me",
  "is",
  "it",
  "to",
  "of",
  "in",
  "on",
  "at",
  "a",
  "an",
  "do",
  "i",
  "you",
  "your",
]);

function words(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s]/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
}

const stem = (w: string) => (w.length > 3 && w.endsWith("s") ? w.slice(0, -1) : w);

function contentTokens(question: string): string[] {
  return words(question)
    .filter((w) => w.length > 2 && !STOP_WORDS.has(w))
    .map(stem);
}

function scoreMemory(tokens: string[], memory: MemoryView): number {
  const haystack = new Set(
    words(`${memory.sentence} ${memory.subject} ${memory.object}`).map(stem),
  );
  return tokens.filter((t) => haystack.has(t)).length;
}

/** Predicates that state when something happens, which is what a "when" question wants. */
const TIME_PREDICATES: ReadonlySet<string> = new Set(["appointmentAt", "dueOn", "swimsOn"]);

/** The one predicate that says who a person is to the household. */
const RELATIONSHIP_PREDICATE = "relationship";

const ABOUT_ENTITY =
  /^\s*(?:who\s+(?:is|was)|who['’]s|tell\s+me\s+about|what\s+about)\s+(?:the\s+)?(.+?)\s*[?.!]*\s*$/i;

const kindFromName = (name: string): ItemKind => {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["jpg", "jpeg", "png", "heic", "webp", "gif"].includes(ext)) return "image";
  if (["mp3", "m4a", "wav", "ogg", "flac"].includes(ext)) return "audio";
  if (["txt", "md"].includes(ext)) return "text";
  return "document";
};

const TITLES: Record<string, string> = { "conflict-dentist": "Your dentist appointment" };
const SUBJECTS: Record<string, string> = { "conflict-dentist": "Dentist" };

export class MockEngine implements EngineClient {
  private readonly clock: Date;
  private items: ItemView[];
  private memories: MemoryView[];
  private entities: EntityView[];
  /** What each rename, merge or move would need to be taken back. */
  private readonly changes = new Map<string, () => void>();
  /** Entities merged into another: kept so a merge can be taken back, never listed. */
  private readonly mergedAway = new Set<string>();
  private pages: PageView[];
  private readonly printStress: PageView;
  private settings: SettingsView;
  private status: EngineStatus;
  private setup: SetupView;
  private readonly listeners = new Map<EngineEventName, Set<(payload: never) => void>>();
  private readonly previews = new Map<
    string,
    { ids: string[]; includeOriginals: boolean; gone: string[] }
  >();
  private counter = 0;
  /** How many times the sound settings were asked for, for tests. */
  soundSettingsOpened = 0;
  /** How many times the desktop's shortcut settings were opened. */
  shortcutSettingsOpened = 0;
  private readonly snoozed = new Set<string>();
  private tierOffer: Tier | null;
  private unfinishedTier: Tier | null;
  /** How many times "Tidy anyway" was pressed. */
  tidyAnywayCount = 0;
  /** Away from home. Tests drive its scripted parts through this. */
  readonly remote: RemoteSim;
  /** Online answers. */
  readonly online = new OnlineSim({ onChange: (v) => this.emit("online-changed", v) });
  /** Choosing the model for each job. */
  readonly models = new ModelsSim();
  /** Backups and restores. Tests drive their next steps through `script`. */
  readonly backups: MockBackups;
  readonly script: MockBackupsScript;
  /** The journal: its state and every journal method. */
  readonly journal: MockJournal;
  /** What paired phone apps bring: nothing until a test or the demo gives it fixtures. */
  readonly phones = new PhonesSim();

  constructor(options: MockOptions = {}) {
    this.firstAsk = options.cold ?? false;
    this.helpWritten = options.helpWritten ?? helpFromAddress();
    this.emphasis = options.emphasis ?? true;
    const raw =
      options.now ?? mockNowFromAddress() ?? (import.meta.env.VITE_MOCK_NOW || DEFAULT_NOW);
    this.clock = new Date(raw);
    const seed = buildSeed(this.clock);
    this.items = seed.items;
    this.memories = seed.memories;
    this.entities = seed.entities;
    this.pages = seed.pages;
    if (options.journalQuestion ?? journalQuestionFromAddress()) {
      const made = journalQuestionSeed(this.clock);
      this.items.push(made.item);
      const today = this.pages.find((p) => p.date === localDay(this.clock));
      if (today) today.question = made.question;
    }
    this.printStress = seed.printStress;
    this.settings = seed.settings;
    this.status = {
      state: "resting",
      needsYou: 0,
      waitingToRead: 0,
      sentToInternetBytes: 0,
      sentMeasured: options.sentMeasured ?? false,
      pausedUntil: null,
      tidy: null,
      nextTidy: "01:00",
      notices: options.notices ?? [],
    };
    this.tierOffer = options.tierOffer ?? null;
    this.unfinishedTier = options.unfinishedTier ?? null;
    this.setup = seed.setup;
    this.remote = new RemoteSim((name, payload) => this.emit(name, payload), Date.now, {
      ...remoteFromAddress(),
      ...options.remote,
    });
    this.backups = new MockBackups({
      counts: () => ({
        memories: this.memories.length,
        pages: this.pages.length,
        originals: this.items.filter((item) => item.hasOriginal).length,
      }),
      now: () => this.clock.getTime(),
      onlineUsed: () => this.online.view().setUp,
      awayUsed: () => this.remote.view().devices.length > 0,
      onProgress: (progress) => this.emit("backup-progress", progress),
    });
    this.script = this.backups.script;
    this.journal = new MockJournal(
      {
        now: () => this.clock,
        lookup: (id) => this.items.find((i) => i.id === id),
        changed: (item) => this.emit("item-changed", item),
      },
      options.journal,
    );
    this.refreshStatus();
  }

  // Events

  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe {
    let set = this.listeners.get(name);
    if (!set) {
      set = new Set();
      this.listeners.set(name, set);
    }
    set.add(cb as (payload: never) => void);
    return () => {
      set?.delete(cb as (payload: never) => void);
    };
  }

  private emit<E extends EngineEventName>(name: E, payload: EngineEvents[E]): void {
    for (const cb of [...(this.listeners.get(name) ?? [])]) cb(clone(payload) as never);
  }

  // Derived state

  private today(): string {
    return localDay(this.clock);
  }

  private todayPage(): PageView {
    const found = this.pages.find((p) => p.date === this.today());
    if (!found) throw new Error("The mock has no page for today");
    return found;
  }

  private openNeeds(): number {
    return this.todayPage().needsYou.filter((l) => !l.done && !l.forgotten).length;
  }

  /** Web pages kept during this session wait for the person to say whether they are true. */
  private waitingUnconfirmed(): MemoryView[] {
    return this.memories.filter(
      (m) => m.id.startsWith("mem-new-") && m.status === "unconfirmed" && !this.snoozed.has(m.id),
    );
  }

  /** Everything waiting in Needs you: the page's lines, web pages to confirm, same-name questions. */
  private needsTotal(): number {
    return (
      this.openNeeds() + this.waitingUnconfirmed().length + this.journal.openQuestions().length
    );
  }

  private refreshStatus(emit = false): void {
    const waiting = this.items.filter((i) => i.status === "kept" || i.status === "reading").length;
    this.status = {
      ...this.status,
      needsYou: this.needsTotal(),
      waitingToRead: waiting,
    };
    if (this.status.state !== "paused") {
      this.status.state = this.items.some((i) => i.status === "reading" && i.id.startsWith("new-"))
        ? "reading"
        : "resting";
    }
    if (emit) this.emit("status-changed", this.status);
  }

  private retitle(): void {
    const page = this.todayPage();
    const n = this.needsTotal();
    page.moreNeedsYou = this.journal.openQuestions().length;
    const words = ["Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"];
    page.headline =
      n === 0
        ? "Nothing needs you today."
        : n === 1
          ? "One thing needs you today."
          : `${words[n - 2] ?? n} things need you today.`;
  }

  private needsChanged(): void {
    this.retitle();
    this.refreshStatus(true);
    this.emit("needs-you-changed", this.status.needsYou);
  }

  // Status and setup

  async getStatus(): Promise<EngineStatus> {
    return clone(this.journal.adjustStatus(this.status));
  }

  async getSetup(): Promise<SetupView> {
    return clone(this.setup);
  }

  async startSetup(tier: Tier): Promise<void> {
    if (tier === this.tierOffer) {
      this.tierOffer = null;
      this.dropNotices("tierDropped");
    }
    if (tier === this.unfinishedTier) {
      this.unfinishedTier = null;
      this.dropNotices("tierUnfinished");
    }
    this.setup = { ...this.setup, phase: "downloading", chosen: tier, paused: null };
    this.emit("setup-progress", this.setup);
  }

  async getTierOffer(): Promise<Tier | null> {
    return this.tierOffer;
  }

  async declineTierOffer(): Promise<void> {
    this.tierOffer = null;
    this.dropNotices("tierDropped");
  }

  async getUnfinishedTier(): Promise<Tier | null> {
    return this.unfinishedTier;
  }

  async declineUnfinishedTier(): Promise<void> {
    this.unfinishedTier = null;
    this.dropNotices("tierUnfinished");
  }

  retryEngineCount = 0;

  async retryEngine(): Promise<void> {
    this.retryEngineCount += 1;
    this.dropNotices("restarts");
  }

  async tidyAnyway(): Promise<void> {
    this.tidyAnywayCount += 1;
    this.dropNotices("tidyPausedBattery");
  }

  private dropNotices(code: NoticeCode): void {
    this.status = { ...this.status, notices: this.status.notices.filter((n) => n.code !== code) };
    this.emit("status-changed", this.status);
  }

  async pauseSetup(): Promise<void> {
    this.setup = { ...this.setup, phase: "paused", paused: "person" };
    this.emit("setup-progress", this.setup);
  }

  async resumeSetup(): Promise<void> {
    this.setup = { ...this.setup, phase: "downloading", paused: null };
    this.emit("setup-progress", this.setup);
  }

  // Keeping

  private newItem(
    kind: ItemKind,
    title: string,
    source: Source,
    extra: Partial<ItemView> = {},
  ): ItemView {
    this.counter += 1;
    const item: ItemView = {
      id: `new-${this.counter}`,
      kind,
      title,
      source,
      trust: source === "pastedWeb" || source === "agent" ? "untrusted" : "trusted",
      keptAt: this.clock.toISOString(),
      status: "kept",
      reason: null,
      durationMs: null,
      pages: null,
      fileName: null,
      hasOriginal: true,
      originalPath: `originals/new-${this.counter}`,
      private: false,
      memoryCount: 0,
      understoodInMs: null,
      summary: null,
      text: null,
      language: null,
      fromDevice: null,
      ...extra,
    };
    this.items.unshift(item);
    this.emit("item-changed", clone(item));
    // The preview has no mind. Only a pasted web page is "read", and it leaves a memory to show for it.
    if (source === "pastedWeb") this.scheduleReading(item.id, true);
    return item;
  }

  private scheduleReading(id: string, web: boolean): void {
    const change = (patch: Partial<ItemView>) => {
      const found = this.items.find((i) => i.id === id);
      if (!found) return;
      Object.assign(found, patch);
      this.refreshStatus(true);
      this.emit("item-changed", found);
    };
    setTimeout(() => change({ status: "reading" }), 1500);
    setTimeout(() => {
      if (web) this.learn(id, true);
      else if (this.items.find((i) => i.id === id)?.text) this.learn(id, false);
      const memoryCount = this.memories.filter((m) =>
        m.receipts.some((r) => r.itemId === id),
      ).length;
      change({
        status: web ? "unconfirmed" : memoryCount > 0 ? "understood" : "kept",
        understoodInMs: memoryCount > 0 ? 3000 : null,
        memoryCount,
      });
    }, 4500);
  }

  /**
   * What was read from a note is remembered in the person's own words. A pasted web page is
   * remembered as unconfirmed and waits on Needs you. Sound and pictures need a mind to read
   * them, so the preview learns nothing from them.
   */
  private learn(itemId: string, unconfirmed: boolean): void {
    const item = this.items.find((i) => i.id === itemId);
    if (!item || this.memories.some((m) => m.id === `mem-new-${itemId}`)) return;
    const first = (item.text ?? item.title).trim().split("\n")[0] ?? item.title;
    const sentence = /[.!?]$/.test(first) ? first : `${first}.`;
    this.memories.push({
      id: `mem-new-${itemId}`,
      sentence,
      subject: item.title,
      entityId: null,
      predicate: "says",
      object: item.title,
      certainty: unconfirmed ? "notSure" : "sure",
      status: unconfirmed ? "unconfirmed" : "active",
      validFrom: null,
      validTo: null,
      due: null,
      learnedAt: item.keptAt,
      lastUsed: null,
      letGoAt: null,
      keepForever: false,
      private: false,
      was: null,
      movedFrom: null,
      receipts: [
        {
          itemId,
          kind: item.kind,
          source: item.source,
          label: item.title,
          keptAt: item.keptAt,
          durationMs: null,
          span: null,
          unconfirmed,
          fromDevice: null,
        },
      ],
    });
    this.refreshStatus(true);
    if (unconfirmed) this.emit("needs-you-changed", this.status.needsYou);
  }

  private firstLine(text: string): string {
    const line = text.trim().split("\n")[0] ?? "";
    return line.length > 60 ? `${line.slice(0, 57)}...` : line;
  }

  async keepText(text: string, source: Source): Promise<Kept> {
    const clean = text.trim();
    const duplicate = this.items.find(
      (i) => i.text !== null && i.text.trim() === clean && i.kind !== "audio",
    );
    if (duplicate) return { type: "duplicate", item: clone(duplicate) };
    const kind: ItemKind = source === "pastedWeb" ? "web" : "text";
    const item = this.newItem(kind, this.firstLine(clean), source, { text: clean });
    this.refreshStatus(true);
    return { type: "new", item: clone(item) };
  }

  async keepFiles(paths: string[], source: Source, entry?: EntryAt): Promise<Kept[]> {
    const kept = paths.map((path) => {
      const name = path.split(/[\\/]/).pop() ?? path;
      // The same file kept twice is the same thing: say so, never add a second copy.
      const duplicate = this.items.find((i) => i.fileName === name);
      if (duplicate) return { type: "duplicate", item: clone(duplicate) } satisfies Kept;
      const item = this.newItem(kindFromName(name), name, source, { fileName: name });
      return { type: "new", item: clone(item) } satisfies Kept;
    });
    if (entry) {
      for (const [n, k] of kept.entries()) {
        await this.journal.placeInEntry(k.item.id, entry.day, entry.block + n);
      }
    }
    return kept;
  }

  async keepBytes(bytes: Uint8Array, mime: string, source: Source, entry?: EntryAt): Promise<Kept> {
    const kind: ItemKind = mime.startsWith("image/")
      ? "image"
      : mime.startsWith("audio/")
        ? "audio"
        : mime.startsWith("text/")
          ? "text"
          : "document";
    const title = {
      image: "Photo",
      audio: "Voice note",
      text: "Note",
      document: "Document",
      web: "Web page",
    }[kind];
    const item = this.newItem(kind, title, source, {
      durationMs: kind === "audio" ? Math.max(1000, Math.round(bytes.length / 16)) : null,
      ...heldOriginal(bytes, mime),
    });
    if (entry) await this.journal.placeInEntry(item.id, entry.day, entry.block);
    return { type: "new", item: clone(item) };
  }

  async unkeep(id: string): Promise<void> {
    const found = this.items.find((i) => i.id === id);
    if (!found && this.journal.unkeep(id)) {
      this.refreshStatus(true);
      return;
    }
    if (!found) throw new Error(`No item ${id}`);
    // Taking back is only for something kept a moment ago, as in the real engine.
    if (this.clock.getTime() - new Date(found.keptAt).getTime() > UNKEEP_MINUTES * 60_000) {
      throw new Error(`Item ${id} was kept too long ago to take back`);
    }
    release(found);
    this.items = this.items.filter((i) => i.id !== id);
    this.memories = this.memories.filter(
      (m) => !(m.receipts.length > 0 && m.receipts.every((r) => r.itemId === id)),
    );
    this.refreshStatus(true);
  }

  async readNow(id: string): Promise<ItemView> {
    const found = this.items.find((i) => i.id === id);
    if (!found) throw new Error(`No item ${id}`);
    found.status = "reading";
    this.scheduleReading(id, found.kind === "web");
    this.emit("item-changed", found);
    return clone(found);
  }

  async setItemPrivate(id: string, isPrivate: boolean): Promise<void> {
    const found = this.items.find((i) => i.id === id);
    if (!found) throw new Error(`No item ${id}`);
    found.private = isPrivate;
  }

  async setItemKeepForever(id: string, keepForever: boolean): Promise<void> {
    for (const m of this.memories.filter((x) => x.receipts.some((r) => r.itemId === id))) {
      m.keepForever = keepForever;
    }
  }

  async keepRecording(bytes: Uint8Array, mime: string): Promise<Kept> {
    // As the app does: the speech model reads WAV, so that is all a recording may be.
    if (mime.split(";")[0]?.trim() !== "audio/wav") {
      throw new Error("a recording must be a WAV file");
    }
    const item = this.newItem("audio", "Voice note", "recording", {
      // 16 kHz mono 16-bit is 32 000 bytes a second after a 44-byte header.
      durationMs: Math.max(1000, Math.round(Math.max(0, bytes.length - 44) / 32)),
      ...heldOriginal(bytes, mime),
    });
    return { type: "new", item: clone(item) };
  }

  async listItems(filter: ItemFilter): Promise<ItemView[]> {
    const since = filter.since ? new Date(filter.since).getTime() : null;
    let out = this.items
      .filter((i) => since === null || new Date(i.keptAt).getTime() >= since)
      .filter((i) => filter.status === null || i.status === filter.status)
      .sort((a, b) => new Date(b.keptAt).getTime() - new Date(a.keptAt).getTime());
    if (filter.limit !== null) out = out.slice(0, filter.limit);
    return clone(out);
  }

  async getItem(id: string): Promise<ItemView> {
    const found = this.items.find((i) => i.id === id) ?? this.journal.findItem(id);
    if (!found) throw new Error(`No item ${id}`);
    const checkin = this.journal.checkinOf(id);
    return clone(checkin ? { ...found, journal: "checkin" as const, checkin } : found);
  }

  async retryItem(id: string): Promise<ItemView> {
    const found = this.items.find((i) => i.id === id);
    if (!found) throw new Error(`No item ${id}`);
    found.status = "reading";
    found.reason = null;
    this.scheduleReading(id, false);
    this.emit("item-changed", found);
    return clone(found);
  }

  // Asking

  private readonly cancelledAsks = new Set<string>();
  private firstAsk: boolean;
  private readonly emphasis: boolean;
  private askUnavailable = false;
  private readonly helpWritten: boolean;

  /** Makes every question answer "unavailable", as when the voice that reads cannot start. */
  setAskUnavailable(on: boolean): void {
    this.askUnavailable = on;
  }

  async cancelAsk(askId: string): Promise<void> {
    this.cancelledAsks.add(askId);
  }

  async appReady(): Promise<void> {}

  async reportRecording(_state: RecordingState): Promise<void> {}

  async ask(question: string, place?: AskPlace, help?: boolean): Promise<string> {
    this.counter += 1;
    const askId = `ask-${this.counter}`;
    this.runAsk(askId, question, place, help ?? false);
    return askId;
  }

  async getGuide(): Promise<GuideView> {
    return mockGuide(pkg.version);
  }

  /**
   * For "who is Leo?" and its kin: the memory that says who the person is to the household first,
   * then up to two more things kept about them, newest first. Nothing is said without a receipt, so
   * a person with no memory that states the relationship gets the facts alone.
   */
  private memoriesAboutEntity(question: string): { m: MemoryView; score: number }[] | null {
    const asked = ABOUT_ENTITY.exec(question)?.[1];
    if (!asked) return null;
    const name = words(asked).join(" ");
    const entity = this.entities.find((e) => {
      if (e.kind === "thing") return false;
      const full = words(e.name).join(" ");
      return full === name || words(e.name)[0] === name;
    });
    if (!entity) return null;
    const first = words(entity.name)[0] as string;
    const mentions = (m: MemoryView) => words(m.sentence).includes(first);
    const mine = this.memories.filter(
      (m) => m.status === "active" && !m.private && (m.entityId === entity.id || mentions(m)),
    );
    const lead = mine.find(
      (m) => m.predicate === RELATIONSHIP_PREDICATE && m.entityId === entity.id,
    );
    const rank = (m: MemoryView) =>
      (words(m.subject)[0] === first ? 1 : 0) * 1e15 + new Date(m.learnedAt).getTime();
    const facts = mine
      .filter((m) => m !== lead)
      .sort((a, b) => rank(b) - rank(a))
      .slice(0, 2);
    const chosen = lead ? [lead, ...facts] : facts;
    return chosen.length > 0 ? chosen.map((m) => ({ m, score: 1 })) : null;
  }

  private runAsk(askId: string, question: string, place?: AskPlace, askedForHelp = false): void {
    const by = answeredBy(place, this.online.view());
    const match = matchGuide(question, askedForHelp);
    const onlineOn = this.online.view().state.type === "on";
    const help =
      match === null
        ? null
        : helpView(
            match,
            match.confident ? (this.helpWritten ? "written" : "words") : "unsure",
            onlineOn,
          );
    const delta = (d: Partial<AskDelta> & Pick<AskDelta, "phase">): AskDelta => ({
      askId,
      textDelta: null,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
      plain: false,
      ...d,
    });
    const terminal = new Set(["done", "notSure", "nothing"]);
    const send = (step: AskDelta): void => {
      if (this.cancelledAsks.has(askId)) return;
      if (!terminal.has(step.phase)) {
        this.emit("ask-delta", step);
        return;
      }
      const d = help && help.kind !== "written" ? { ...step, help } : step;
      if (by.type !== "online") {
        this.emit("ask-delta", { ...d, answeredBy: by, sendId: null });
        return;
      }
      // Nothing to send means nothing goes: the question is noted as not sent, kept here.
      const facts = (d.phase === "done" ? active : d.phase === "notSure" ? unconfirmed : []).map(
        (x) => x.m.sentence,
      );
      if (facts.length === 0) {
        this.online.record(question, d.phase, facts);
        this.emit("ask-delta", {
          ...d,
          answeredBy: { type: "computer", reason: "keptHere" },
          sendId: null,
        });
        return;
      }
      const sendId = this.online.record(question, d.phase, facts);
      this.emit("ask-delta", { ...d, answeredBy: by, sendId });
    };
    const tokens = contentTokens(question);
    const asksWhen = /^\s*when\b/i.test(question);
    const scored = (status: MemoryView["status"]) => {
      const matches = this.memories
        .filter((m) => m.status === status && !m.private)
        .map((m) => ({ m, score: scoreMemory(tokens, m) }))
        .filter((x) => tokens.length > 0 && x.score / tokens.length >= 0.5)
        .sort((a, b) => b.score - a.score);
      return asksWhen ? matches.filter((x) => TIME_PREDICATES.has(x.m.predicate)) : matches;
    };
    const aboutEntity = this.memoriesAboutEntity(question);
    const active = aboutEntity ?? scored("active").slice(0, 2);
    const unconfirmed = scored("unconfirmed").slice(0, 2);

    // A help answer is always written here, from the guide and what was kept: nothing goes
    // online and nothing is noted as sent.
    if (help?.kind === "written" && match && !this.askUnavailable) {
      const kept = active.slice(0, 1);
      const parts = [
        ...match.ids.map((id) => writtenFor(id)),
        ...kept.map((x) => (this.emphasis ? emphasise(x.m.sentence, x.m.object) : x.m.sentence)),
      ];
      const done: AskDelta = delta({
        phase: "done",
        receipts: kept.flatMap((x) => x.m.receipts),
        memoryIds: kept.map((x) => x.m.id),
        help: {
          ...help,
          sections: match.ids.flatMap((id) => cite(id) ?? []),
          cited: [...match.ids, ...kept.map((x) => x.m.id)],
        },
      });
      const steps = [
        delta({ phase: "searching" }),
        delta({ phase: "reading", found: kept.length }),
        ...parts
          .join(" ")
          .split(/(?<=\s)/)
          .map((piece) => delta({ phase: "streaming", textDelta: piece })),
        done,
      ];
      const next = (i: number): void => {
        const step = steps[i];
        if (!step) return;
        setTimeout(() => {
          if (this.cancelledAsks.has(askId)) return;
          this.emit("ask-delta", step);
          next(i + 1);
        }, 30);
      };
      next(0);
      return;
    }

    if (this.askUnavailable) {
      setTimeout(() => send(delta({ phase: "unavailable" })), 0);
      return;
    }
    // The first question after the app starts wakes the voice that reads, and says so.
    if (this.firstAsk) {
      this.firstAsk = false;
      setTimeout(() => send(delta({ phase: "waking", etaSeconds: WAKE_SECONDS })), 0);
    }
    setTimeout(() => send(delta({ phase: "searching" })), 0);
    for (const phase of onlinePrelude(by)) {
      setTimeout(() => send(delta({ phase, found: active.length })), 100);
    }
    setTimeout(() => {
      if (active.length > 0) {
        const receipts = active.flatMap((x) => x.m.receipts);
        send(
          delta({
            phase: "reading",
            found: active.length,
            receipts,
            memoryIds: active.map((x) => x.m.id),
          }),
        );
        const text = active
          .map((x) => (this.emphasis ? emphasise(x.m.sentence, x.m.object) : x.m.sentence))
          .join(" ");
        const pieces = text.split(/(?<=\s)/);
        const steps = [
          ...pieces.map((piece) => delta({ phase: "streaming", textDelta: piece })),
          delta({ phase: "done", memoryIds: active.map((x) => x.m.id) }),
        ];
        // One timer at a time, so a busy machine can delay a piece but never reorder two.
        const next = (i: number): void => {
          const step = steps[i];
          if (!step) return;
          setTimeout(() => {
            send(step);
            next(i + 1);
          }, 30);
        };
        next(0);
      } else if (unconfirmed.length > 0) {
        send(
          delta({
            phase: "notSure",
            near: unconfirmed.map((x) => ({
              text: x.m.sentence,
              receipt: x.m.receipts[0] as Receipt,
            })),
          }),
        );
      } else {
        send(delta({ phase: "nothing", found: 0 }));
      }
    }, 250);
  }

  // Pages

  async getPage(date?: string): Promise<PageView> {
    const day = date ?? this.today();
    if (day === this.today()) this.retitle();
    const found = this.pages.find((p) => p.date === day);
    const journalLine = this.journal.pageLine(day);
    if (found) return { ...clone(found), ...(journalLine ? { journalLine } : {}) };
    const quiet: PageView = {
      ...clone(this.pages[this.pages.length - 1] as PageView),
      date: day,
      pageNumber: 1,
      variant: "quiet",
      headline: "A quiet night. Nothing needs you.",
      dek: "Nothing new was kept yesterday.",
      needsYou: [],
      changed: [],
      letGo: [],
      question: null,
      keptCount: 0,
      changesCount: 0,
      letGoCount: 0,
      pending: 0,
    };
    return quiet;
  }

  async listPages(): Promise<PageSummary[]> {
    return [...this.pages]
      .sort((a, b) => b.date.localeCompare(a.date))
      .map((p) => ({
        date: p.date,
        pageNumber: p.pageNumber,
        headline: p.headline,
        needsYou: p.needsYou.filter((l) => !l.done && !l.forgotten).length,
      }));
  }

  private lines(): PageLine[] {
    return this.pages.flatMap((p) => [
      ...p.needsYou,
      ...p.changed,
      ...p.letGo,
      ...p.upcoming,
      ...p.learned,
    ]);
  }

  private memory(id: string): MemoryView {
    const found = this.memories.find((m) => m.id === id);
    if (!found) throw new Error(`No memory ${id}`);
    return found;
  }

  async answerQuestion(pageDate: string, choice: QuestionChoice): Promise<void> {
    const page = this.pages.find((p) => p.date === pageDate);
    if (!page?.question) return;
    page.question.answered = choice;
    const asked = page.question.journal;
    if (
      asked &&
      !asked.answerItemId &&
      (choice === "fine" || choice === "notGreat" || choice === "notYet")
    ) {
      asked.answerItemId = this.journal.keepAnswer(choice, asked.itemId, page.question.text);
    }
    const memoryId = page.question.memoryId;
    if (memoryId) {
      const m = this.memory(memoryId);
      if (choice === "yes") m.certainty = "sure";
      if (choice === "no") m.status = "letGo";
    }
    if (choice === "earlier" || choice === "later") {
      const [h, min] = this.settings.pageTime.split(":").map(Number) as [number, number];
      const total = h * 60 + min + (choice === "earlier" ? -30 : 30);
      const next = ((total % 1440) + 1440) % 1440;
      this.settings.pageTime = `${String(Math.floor(next / 60)).padStart(2, "0")}:${String(next % 60).padStart(2, "0")}`;
    }
  }

  async addAnswerLine(pageDate: string, line: string): Promise<void> {
    const question = this.pages.find((p) => p.date === pageDate)?.question;
    const asked = question?.journal;
    if (!asked) return;
    asked.line = line;
    if (asked.answerItemId) this.journal.setAnswerLine(asked.answerItemId, line);
  }

  async resolveWhich(conflictId: string, memoryId: string): Promise<WhichResolved> {
    let resolved: WhichResolved | null = null;
    for (const line of this.lines().filter((l) => l.conflictId === conflictId)) {
      const chosen = line.choices.find((c) => c.memoryId === memoryId);
      if (!chosen) continue;
      line.done = true;
      resolved = { subject: SUBJECTS[conflictId] ?? "This", chosen: chosen.text, memoryId };
      line.text = `${TITLES[conflictId] ?? "This"}: ${chosen.text}.`;
      line.memoryIds = [memoryId];
      line.receipts = [chosen.receipt];
      for (const c of line.choices) {
        const m = this.memory(c.memoryId);
        if (c.memoryId === memoryId) {
          m.status = "active";
          m.certainty = "sure";
        } else {
          m.status = "letGo";
          m.letGoAt = this.clock.toISOString();
        }
      }
    }
    this.needsChanged();
    if (!resolved) throw new Error(`No choice ${memoryId} in ${conflictId}`);
    return resolved;
  }

  async confirmMemory(id: string, confirmed: boolean): Promise<void> {
    if (confirmed) {
      const m = this.memory(id);
      m.status = "active";
      m.certainty = "sure";
    } else {
      // As the real engine does: saying it is not true leaves it unconfirmed, never deleted.
      const m = this.memory(id);
      m.status = "unconfirmed";
      m.certainty = "notSure";
      m.letGoAt = null;
      this.snoozed.delete(id);
    }
    this.needsChanged();
  }

  async markDone(memoryId: string): Promise<void> {
    for (const line of this.lines().filter((l) => l.memoryIds.includes(memoryId))) {
      line.done = true;
    }
    const m = this.memory(memoryId);
    m.due = null;
    this.needsChanged();
  }

  // Memories

  async listMemories(filter: MemoryFilter): Promise<MemoryView[]> {
    const q = filter.query?.toLowerCase() ?? null;
    let out = this.memories
      .filter((m) => filter.entityId === null || m.entityId === filter.entityId)
      .filter((m) => filter.itemId === null || m.receipts.some((r) => r.itemId === filter.itemId))
      .filter((m) => filter.status === null || m.status === filter.status)
      .filter((m) => q === null || `${m.sentence} ${m.subject}`.toLowerCase().includes(q));
    if (filter.limit !== null) out = out.slice(0, filter.limit);
    return clone(out);
  }

  private liveEntities(): EntityView[] {
    return this.entities.filter((e) => !this.mergedAway.has(e.id));
  }

  /** The entity as a screen sees it: counts from the memories filed under it now. */
  private entityView(e: EntityView): EntityView {
    const mine = this.memories.filter((m) => m.entityId === e.id && m.status !== "letGo");
    const items = new Set(mine.flatMap((m) => m.receipts.map((r) => r.itemId)));
    return clone({ ...e, memoryCount: mine.length, itemCount: items.size });
  }

  private entityFor(id: string): EntityView {
    const found = this.liveEntities().find((e) => e.id === id);
    if (!found) throw new Error(`No entity ${id}`);
    return found;
  }

  private nextChange(undo: () => void): string {
    this.counter += 1;
    const id = `chg-${this.counter}`;
    this.changes.set(id, undo);
    return id;
  }

  /** Words of a memory that carry a name follow it, as the real engine does. */
  private reword(entityId: string, from: string, to: string): string[] {
    const changed: string[] = [];
    const pattern = new RegExp(
      `(?<![\\p{L}\\p{N}_])${escapeRegExp(from)}(?![\\p{L}\\p{N}_])`,
      "gu",
    );
    for (const m of this.memories.filter((x) => x.entityId === entityId)) {
      const next = m.sentence.replace(pattern, () => to);
      if (next !== m.sentence) {
        m.sentence = next;
        changed.push(m.id);
      }
    }
    return changed;
  }

  async getEntity(id: string): Promise<EntityView> {
    return this.entityView(this.entityFor(id));
  }

  async listEntities(kind: EntityKind | null, query: string | null): Promise<EntityView[]> {
    const q = query?.trim().toLowerCase() ?? "";
    return this.liveEntities()
      .map((e) => this.entityView(e))
      .filter((e) => e.memoryCount > 0)
      .filter((e) => kind === null || e.kind === kind)
      .filter(
        (e) =>
          q === "" ||
          `${e.name} ${e.role ?? ""} ${e.formerNames.join(" ")}`.toLowerCase().includes(q),
      );
  }

  async renameEntity(id: string, name: string): Promise<RenameOutcome> {
    const e = this.entityFor(id);
    const clean = name.trim().split(/\s+/).join(" ");
    if (clean === "" || clean.length > 120) throw new Error("A name needs some letters.");
    const other = this.liveEntities().find(
      (o) => o.id !== id && o.name.toLowerCase() === clean.toLowerCase(),
    );
    if (other) return { type: "exists", entity: this.entityView(other) };
    if (clean === e.name) return { type: "renamed", entity: this.entityView(e), changeId: null };
    const before = e.name;
    e.name = clean;
    e.formerNames = [before, ...e.formerNames.filter((n) => n !== before && n !== clean)];
    for (const m of this.memories.filter((x) => x.entityId === id)) m.subject = clean;
    const reworded = this.reword(id, before, clean);
    const changeId = this.nextChange(() => {
      for (const m of this.memories.filter((x) => reworded.includes(x.id))) {
        m.sentence = m.sentence.split(clean).join(before);
      }
      for (const m of this.memories.filter((x) => x.entityId === id)) m.subject = before;
      e.name = before;
      e.formerNames = e.formerNames.filter((n) => n !== before && n !== clean);
    });
    e.lastChange = { id: changeId, kind: "rename", from: before, at: this.clock.toISOString() };
    return { type: "renamed", entity: this.entityView(e), changeId };
  }

  async mergeEntities(fromId: string, intoId: string): Promise<MergeOutcome> {
    if (fromId === intoId) throw new Error("A thing cannot be merged with itself.");
    const gone = this.entityFor(fromId);
    const kept = this.entityFor(intoId);
    const moved = this.memories.filter((m) => m.entityId === fromId);
    const before = new Map(moved.map((m) => [m.id, { subject: m.subject, sentence: m.sentence }]));
    for (const m of moved) {
      m.entityId = intoId;
      m.subject = kept.name;
    }
    const reworded = this.reword(intoId, gone.name, kept.name).filter((id) => before.has(id));
    this.mergedAway.add(fromId);
    const previousNames = kept.formerNames;
    kept.formerNames = [gone.name, ...gone.formerNames, ...kept.formerNames];
    const previousChange = kept.lastChange;
    const changeId = this.nextChange(() => {
      for (const m of moved) {
        const was = before.get(m.id);
        if (!was) continue;
        m.entityId = fromId;
        m.subject = was.subject;
        if (reworded.includes(m.id)) m.sentence = was.sentence;
      }
      this.mergedAway.delete(fromId);
      kept.formerNames = previousNames;
      kept.lastChange = previousChange;
    });
    kept.lastChange = {
      id: changeId,
      kind: "merge",
      from: gone.name,
      at: this.clock.toISOString(),
    };
    return { entity: this.entityView(kept), changeId, moved: moved.length };
  }

  async refileMemory(id: string, target: RefileTarget): Promise<Refiled> {
    const m = this.memory(id);
    const fromId = m.entityId;
    let created = false;
    let dest: EntityView;
    if (target.type === "existing") {
      dest = this.entityFor(target.entityId);
    } else {
      const name = target.name.trim().split(/\s+/).join(" ");
      if (name === "") throw new Error("A name needs some letters.");
      const found = this.liveEntities().find((o) => o.name.toLowerCase() === name.toLowerCase());
      if (found) {
        dest = found;
      } else {
        this.counter += 1;
        dest = {
          id: `ent-new-${this.counter}`,
          kind: target.kind,
          name,
          role: null,
          memoryCount: 0,
          itemCount: 0,
          formerNames: [],
          lastChange: null,
        };
        this.entities.push(dest);
        created = true;
      }
    }
    if (dest.id === fromId) throw new Error("It is already filed there.");
    const before = { subject: m.subject, movedFrom: m.movedFrom };
    m.entityId = dest.id;
    m.subject = dest.name;
    m.movedFrom = m.movedFrom ?? before.subject;
    const changeId = this.nextChange(() => {
      m.entityId = fromId;
      m.subject = before.subject;
      m.movedFrom = before.movedFrom;
      if (created && !this.memories.some((x) => x.entityId === dest.id)) {
        this.entities = this.entities.filter((e) => e.id !== dest.id);
      }
    });
    return { memory: clone(m), entity: this.entityView(dest), changeId };
  }

  async undoChange(changeId: string): Promise<void> {
    const undo = this.changes.get(changeId);
    if (!undo) throw new Error(`That change cannot be taken back.`);
    this.changes.delete(changeId);
    undo();
    for (const e of this.entities) {
      if (e.lastChange?.id === changeId) e.lastChange = null;
    }
  }

  async snooze(memoryId: string, _days: number): Promise<void> {
    const m = this.memory(memoryId);
    if (m.status === "unconfirmed") {
      this.snoozed.add(memoryId);
      this.needsChanged();
      return;
    }
    if (m.status === "fading") m.status = "active";
    m.lastUsed = this.clock.toISOString();
  }

  async correctMemory(id: string, text: string): Promise<MemoryView> {
    const m = this.memory(id);
    m.sentence = text;
    m.certainty = "sure";
    return clone(m);
  }

  async setKeepForever(id: string, keepForever: boolean): Promise<void> {
    this.memory(id).keepForever = keepForever;
  }

  async setPrivate(id: string, isPrivate: boolean): Promise<void> {
    this.memory(id).private = isPrivate;
  }

  async restore(ids: string[]): Promise<void> {
    for (const id of ids) {
      const m = this.memory(id);
      m.status = "active";
      m.letGoAt = null;
      m.lastUsed = this.clock.toISOString();
      for (const line of this.lines().filter(
        (l) => l.kind === "letGo" && l.memoryIds.includes(id),
      )) {
        line.done = true;
      }
    }
  }

  async previewForget(
    ids: string[],
    includeOriginals: boolean,
    items?: ForgetItems,
  ): Promise<ForgetPreview> {
    this.counter += 1;
    const token = `forget-${this.counter}`;
    const rows = ids.map((id) => ({ memoryId: id, sentence: this.memory(id).sentence }));
    const read = items?.pick ? this.journalMemories(items.also) : [];
    for (const m of read) rows.push({ memoryId: m.memoryId, sentence: m.sentence });
    const itemIds = new Set(
      this.memories
        .filter((m) => ids.includes(m.id))
        .flatMap((m) => m.receipts.map((r) => r.itemId)),
    );
    for (const also of items?.also ?? []) itemIds.add(also);
    for (const m of read) for (const cited of m.cites) itemIds.add(cited);
    const originals = includeOriginals
      ? [...itemIds].flatMap((itemId) => {
          const item = this.items.find((i) => i.id === itemId) ?? this.journal.findItem(itemId);
          if (!item) return [];
          const others = this.memories.filter(
            (m) => !ids.includes(m.id) && m.receipts.some((r) => r.itemId === itemId),
          ).length;
          return [
            {
              itemId,
              label: item.title,
              kind: item.kind,
              keptAt: item.keptAt,
              durationMs: item.durationMs,
              removable: others === 0,
              otherMemories: others,
            },
          ];
        })
      : [];
    const gone = originals
      .filter((o) => o.removable && (!items?.pick || items.also.includes(o.itemId)))
      .map((o) => o.itemId);
    this.previews.set(token, {
      ids: [...ids, ...read.map((m) => m.memoryId)],
      includeOriginals,
      gone,
    });
    const cited = this.journal.citedLines(gone);
    const pagesAffected = this.pages.filter((p) =>
      [...p.needsYou, ...p.changed, ...p.letGo].some((l) =>
        l.memoryIds.some((m) => ids.includes(m)),
      ),
    ).length;
    return {
      token,
      memories: rows,
      originals: originals.filter((o) => o.removable),
      includeOriginals,
      pagesAffected,
      reflectionLines: cited.lines,
      recapLines: cited.recap,
    };
  }

  /**
   * What the journal's own items yielded, as sentences, for "Delete this day". A sentence cites
   * its item and, for the entry's second paragraph, a note from the day before.
   */
  private journalMemories(
    also: string[],
  ): Array<{ memoryId: string; sentence: string; cites: string[] }> {
    const SHARED: Record<string, string[]> = { "j-block-2": ["j-wed-gran"] };
    return also.flatMap((id) => {
      const item = this.journal.findItem(id);
      const text = item?.text ?? item?.summary;
      if (!item || item.kind === "checkin" || !text) return [];
      const sentence = text.split(/(?<=[.!?])\s+/)[0] ?? text;
      return [{ memoryId: `jm-${id}`, sentence, cites: SHARED[id] ?? [] }];
    });
  }

  private removeMemories(ids: string[], redact: boolean): { pages: number } {
    this.memories = this.memories.filter((m) => !ids.includes(m.id));
    let pages = 0;
    for (const p of this.pages) {
      let touched = false;
      for (const l of [...p.needsYou, ...p.changed, ...p.letGo]) {
        if (l.memoryIds.some((m) => ids.includes(m))) {
          if (redact) {
            l.forgotten = true;
            l.text = "";
            l.receipts = [];
            l.choices = [];
          }
          touched = true;
        }
      }
      if (touched) pages += 1;
    }
    return { pages };
  }

  async forget(previewToken: string): Promise<ForgetReport> {
    const preview = this.previews.get(previewToken);
    if (!preview) throw new Error("That preview has expired");
    this.previews.delete(previewToken);
    const { pages } = this.removeMemories(preview.ids, true);
    let originals = 0;
    if (preview.includeOriginals) {
      for (const itemId of preview.gone) {
        const stillUsed = this.memories.some((m) => m.receipts.some((r) => r.itemId === itemId));
        if (stillUsed) continue;
        for (const gone of this.items.filter((i) => i.id === itemId)) release(gone);
        const before = this.items.length;
        this.items = this.items.filter((i) => i.id !== itemId);
        if (this.items.length < before || this.journal.delete(itemId)) originals += 1;
      }
    }
    this.needsChanged();
    return {
      memoriesRemoved: preview.ids.length,
      originalsRemoved: originals,
      pagesRedacted: pages,
    };
  }

  async listNeedsYou(): Promise<NeedsYouEntry[]> {
    const out: NeedsYouEntry[] = [];
    for (const line of this.todayPage().needsYou) {
      if (line.kind === "which" && line.conflictId) {
        if (line.done) continue;
        out.push({
          type: "which",
          conflictId: line.conflictId,
          title: TITLES[line.conflictId] ?? line.text,
          options: line.choices.map((c) => ({
            memoryId: c.memoryId,
            text: c.text,
            receipt: c.receipt,
          })),
        });
      } else if (line.kind === "due" && line.memoryIds[0]) {
        const memory = this.memory(line.memoryIds[0]);
        out.push({
          type: "due",
          memory: clone(memory),
          due: memory.due ?? this.today(),
          done: line.done,
        });
      } else if (line.kind === "promise" && line.memoryIds[0]) {
        out.push({
          type: "promise",
          memory: clone(this.memory(line.memoryIds[0])),
          done: line.done,
        });
      }
    }
    for (const memory of this.waitingUnconfirmed()) {
      out.push({ type: "unconfirmed", memory: clone(memory), agreeingSources: 0, used: false });
    }
    for (const question of this.journal.openQuestions()) out.push({ type: "sameName", question });
    return out;
  }

  async listFading(): Promise<MemoryView[]> {
    return clone(this.memories.filter((m) => m.status === "fading" || m.status === "letGo"));
  }

  // The journal, held by `MockJournal`.
  getJournalDay: EngineClient["getJournalDay"] = (day) => this.journal.getJournalDay(day);
  listJournalDays: EngineClient["listJournalDays"] = () => this.journal.listJournalDays();
  listPeriods: EngineClient["listPeriods"] = (kind) => this.journal.listPeriods(kind);
  dismissJournalIntro: EngineClient["dismissJournalIntro"] = () =>
    this.journal.dismissJournalIntro();
  saveEntryText: EngineClient["saveEntryText"] = (day, itemId, block, text) =>
    this.journal.saveEntryText(day, itemId, block, text);
  placeInEntry: EngineClient["placeInEntry"] = (itemId, day, block) =>
    this.journal.placeInEntry(itemId, day, block);
  describePhoto: EngineClient["describePhoto"] = (itemId, text) =>
    this.journal.describePhoto(itemId, text);
  readEntryNow: EngineClient["readEntryNow"] = (day) => this.journal.readEntryNow(day);
  keepNote: EngineClient["keepNote"] = (text, source, feeling, good) =>
    this.journal.keepNote(text, source, feeling, good);
  topFeelingWords: EngineClient["topFeelingWords"] = (part) => this.journal.topFeelingWords(part);
  keepCheckIn: EngineClient["keepCheckIn"] = (input) => this.journal.keepCheckIn(input);
  markTried: EngineClient["markTried"] = (checkinId, idea) =>
    this.journal.markTried(checkinId, idea);
  keepGoodThing: EngineClient["keepGoodThing"] = (day, text, photoItemId) =>
    this.journal.keepGoodThing(day, text, photoItemId);
  setLeftOut: EngineClient["setLeftOut"] = (itemId, leftOut) =>
    this.journal.setLeftOut(itemId, leftOut);
  moveToDay: EngineClient["moveToDay"] = (itemId, day) => this.journal.moveToDay(itemId, day);
  confirmMention: EngineClient["confirmMention"] = (mentionId, role) =>
    this.journal.confirmMention(mentionId, role);
  removeMention: EngineClient["removeMention"] = (mentionId) =>
    this.journal.removeMention(mentionId);
  restoreMention: EngineClient["restoreMention"] = (mentionId) =>
    this.journal.restoreMention(mentionId);
  refileMention: EngineClient["refileMention"] = (mentionId, target) =>
    this.journal.refileMention(mentionId, target);
  answerSameName: EngineClient["answerSameName"] = async (questionId, answer) => {
    const done = await this.journal.answerSameName(questionId, answer);
    this.refreshStatus(true);
    this.emit("needs-you-changed", this.status.needsYou);
    return done;
  };
  entityJournalDays: EngineClient["entityJournalDays"] = (entityId) =>
    this.journal.entityJournalDays(entityId);
  getReflection: EngineClient["getReflection"] = (period) => this.journal.getReflection(period);
  reflect: EngineClient["reflect"] = (period) => this.journal.reflect(period);
  stopReflecting: EngineClient["stopReflecting"] = (period) => this.journal.stopReflecting(period);
  deleteReflection: EngineClient["deleteReflection"] = (period) =>
    this.journal.deleteReflection(period);

  // From your phones

  getPhoneSources = async (): Promise<PhoneSourcesView> => this.phones.view();
  setPhoneSource = async (
    deviceId: string,
    source: PhoneSource,
    taking: boolean,
  ): Promise<PhoneSourcesView> => this.phones.setSource(deviceId, source, taking);
  setPhonePageOptions = async (options: PhonePageOptions): Promise<PhoneSourcesView> =>
    this.phones.setPageOptions(options);
  setNoticing = async (on: boolean): Promise<void> => this.phones.setNoticing(on);
  listCameIn = async (
    deviceId: string,
    source: PhoneSource,
    before: string | null,
    limit: number,
  ): Promise<CameInRow[]> => this.phones.cameIn(deviceId, source, before, limit);
  getHealthSeries = async (kind: ReadingKind, from: string, to: string): Promise<HealthSeries> =>
    this.phones.series(kind, from, to);
  listReadings = async (kind: ReadingKind, from: string, to: string): Promise<ReadingRow[]> =>
    this.phones.readings(kind, from, to);
  previewPhoneDelete = async (scope: PhoneDeleteScope): Promise<PhoneDeletePreview> =>
    this.phones.previewDelete(scope);
  deletePhoneData = async (token: string, _alsoStop: boolean): Promise<void> =>
    this.phones.delete(token);

  // System

  async getPrivacy(): Promise<PrivacyView> {
    const seed = privacySeed(this.clock);
    const { ledger, ...remote } = this.remote.privacy();
    return {
      ...seed,
      ledger: [...seed.ledger, ...ledger],
      sentToInternetBytes: this.status.sentToInternetBytes,
      sentMeasured: this.status.sentMeasured,
      lock: this.settings.lock,
      checkUpdates: this.settings.checkUpdates,
      ...remote,
      ...this.online.privacy(),
    };
  }

  async exportTo(dir: string): Promise<ExportReport> {
    const report: ExportReport = {
      path: `${dir}/Nocturne export`,
      memories: this.memories.length,
      pages: this.pages.length,
      originals: this.items.filter((i) => i.hasOriginal).length,
      notPrivate: false,
    };
    const total = report.memories + report.pages + report.originals;
    for (const done of [Math.round(total / 3), Math.round((2 * total) / 3), total]) {
      setTimeout(() => this.emit("export-progress", { done, total }), done);
    }
    return report;
  }

  async deleteEverything(confirmPhrase: string): Promise<void> {
    if (confirmPhrase !== "delete everything") throw new Error("The confirmation did not match");
    for (const item of this.items) release(item);
    this.items = [];
    this.memories = [];
    for (const p of this.pages) {
      p.needsYou = [];
      p.changed = [];
      p.letGo = [];
      p.question = null;
    }
    this.needsChanged();
  }

  // Backups and restores

  async estimateBackup(): Promise<number> {
    return this.backups.estimate();
  }

  makeBackup = (path: string, passphrase: string): Promise<BackupReport> =>
    this.backups.makeBackup(path, passphrase);

  async cancelBackup(): Promise<void> {
    this.backups.cancel();
  }

  async getBackups(): Promise<BackupsView> {
    return this.backups.view();
  }

  async dismissBackupNotices(): Promise<void> {
    this.backups.dismissNotices();
  }

  probeBackup = (path: string): Promise<void> => this.backups.probeBackup(path);
  checkBackup = (path: string, passphrase: string): Promise<RestoreCheck> =>
    this.backups.checkBackup(path, passphrase);
  restoreBackup = (path: string, passphrase: string, replace: boolean): Promise<void> =>
    this.backups.restoreBackup(path, passphrase, replace);
  restorePutAside = (id: string): Promise<void> => this.backups.restorePutAside(id);
  deletePutAside = (id: string): Promise<void> => this.backups.deletePutAside(id);
  async getRestored(): Promise<RestoredView | null> {
    return this.backups.getRestored();
  }
  async finishRestore(): Promise<void> {
    this.backups.finishRestore();
  }
  async discardRestore(): Promise<void> {
    this.backups.discardRestore();
  }

  async getSettings(): Promise<SettingsView> {
    return clone(this.settings);
  }

  async updateSettings(patch: SettingsPatch): Promise<SettingsView> {
    const next = { ...this.settings } as Record<string, unknown>;
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) continue;
      // Like the app: an empty folder clears it.
      next[key] = key === "inboxFolder" && value === "" ? null : value;
    }
    this.settings = next as unknown as SettingsView;
    return clone(this.settings);
  }

  async pause(until: string | null): Promise<void> {
    this.status = { ...this.status, state: "paused", pausedUntil: until };
    this.emit("status-changed", this.status);
  }

  async resume(): Promise<void> {
    this.status = { ...this.status, state: "resting", pausedUntil: null };
    this.refreshStatus(true);
  }

  async getAdvanced(): Promise<AdvancedView> {
    return {
      ...advancedSeed({ memories: this.memories.length, items: this.items.length }),
      remote: this.remote.advanced(),
      online: this.online.advanced(),
    };
  }

  async getEdition(date: string, edition: Edition): Promise<PageView> {
    const page = date === PRINT_STRESS_DATE ? clone(this.printStress) : await this.getPage(date);
    page.edition = edition;
    if (edition === "screen") return page;
    const hidden = new Set(this.memories.filter((m) => m.private).map((m) => m.id));
    const open = (line: PageLine) => !line.memoryIds.some((id) => hidden.has(id));
    let skipped = 0;
    let droppedLetGo = 0;
    const keep = (lines: PageLine[]) =>
      lines.filter((line) => {
        const ok = open(line);
        if (!ok) skipped += 1;
        return ok;
      });
    page.needsYou = keep(page.needsYou);
    page.changed = keep(page.changed);
    const letGoBefore = page.letGo.length;
    page.letGo = keep(page.letGo);
    droppedLetGo = letGoBefore - page.letGo.length;
    page.upcoming = keep(page.upcoming);
    page.learned = keep(page.learned);
    if (page.question?.memoryId && hidden.has(page.question.memoryId)) {
      page.question = null;
      skipped += 1;
    }
    page.privateSkipped += skipped;
    if (skipped > 0) {
      // The stored headline and dek were written with the private lines in view. Like the
      // engine, never hand them out: fall back to the template, from the counts that remain.
      const needs = page.needsYou.filter((l) => !l.forgotten).length + page.moreNeedsYou;
      page.headline = formatLongDay(page.date);
      page.dek = templateDek(page.keptCount, needs, Math.max(0, page.letGoCount - droppedLetGo));
    }
    return page;
  }

  /**
   * Today's page is read by the real voice, recorded once with the real script and kept as data
   * beside the mock. Any other page is silence with the same parts, so the player still works.
   */
  async speakPage(date: string): Promise<SpokenPage> {
    const page = await this.getEdition(date, "audio");
    const kinds = audioScript(page).map((s) => s.kind);
    const known = recorded.sections.map((s) => s.kind);
    const isRecorded = date === this.today() && kinds.join() === known.join();
    if (isRecorded) {
      return {
        path: (await recordedPageUrl()) ?? mockSpokenUrl(recorded.durationMs),
        sections: recorded.sections.map((s) => ({
          kind: s.kind as SpokenKind,
          startMs: s.startMs,
        })),
        durationMs: recorded.durationMs,
        privateSkipped: page.privateSkipped,
      };
    }
    const durationMs = kinds.length * MOCK_SECTION_MS;
    return {
      path: mockSpokenUrl(durationMs),
      sections: kinds.map((kind, i) => ({ kind, startMs: i * MOCK_SECTION_MS })),
      durationMs,
      privateSkipped: page.privateSkipped,
    };
  }

  async openSoundSettings(): Promise<void> {
    this.soundSettingsOpened += 1;
  }

  async quickKeepCommand(): Promise<string> {
    return "nocturne --quick-capture";
  }

  async configureShortcut(): Promise<void> {
    this.shortcutSettingsOpened += 1;
  }

  async retryShortcut(): Promise<void> {}

  // Away from home

  getRemote = async (): Promise<RemoteView> => this.remote.view();
  startRemote = (connection: RemoteConnection, allowDownload: boolean): Promise<RemoteView> =>
    this.remote.start(connection, allowDownload);
  stopRemote = (removeAll: boolean): Promise<RemoteView> => this.remote.stop(removeAll);
  pauseRemoteDownload = (): Promise<RemoteView> => this.remote.pauseDownload();
  beginPairing = (): Promise<PairingOffer> => this.remote.beginPairing();
  decidePairing = (pairingId: string, allow: boolean): Promise<void> =>
    this.remote.decide(pairingId, allow);
  cancelPairing = (): Promise<void> => this.remote.cancelPairing();
  renameDevice = (id: string, name: string): Promise<DeviceView> => this.remote.rename(id, name);
  removeDevice = (id: string, _alsoDeleteBrought?: boolean): Promise<void> =>
    this.remote.remove(id);
  listRemoteEvents = async (limit: number): Promise<RemoteEvent[]> => this.remote.events(limit);
  updateRemoteSettings = (patch: RemoteSettingsPatch): Promise<RemoteView> =>
    this.remote.update(patch);
  openRemoteSignIn = (): Promise<void> => this.remote.openSignIn();
  newComputerKey = (): Promise<RemoteView> => this.remote.newKey();

  // A phone's memory moving here

  private moving: MoveView | null = moveFromAddress();

  /** Tests and stories: a phone's memory, as the computer would show it. */
  setMove(view: MoveView | null): void {
    this.moving = view;
    this.emit("move-changed", view);
  }

  getMove = async (): Promise<MoveView | null> => clone(this.moving);
  answerMove = async (choice: MoveChoice): Promise<void> => {
    const now = this.moving;
    if (now?.phase !== "offered") throw new Error("There is no offer to answer");
    this.setMove(
      choice === "move" ? { phase: "arriving", name: now.name, done: 0, size: now.bytes } : null,
    );
  };
  finishMove = async (): Promise<void> => {
    if (this.moving?.phase !== "arrived" && this.moving?.phase !== "failed")
      throw new Error("No memory is waiting");
    this.setMove(null);
  };
  stopMove = async (): Promise<void> => {
    const now = this.moving;
    if (now?.phase !== "arriving" && now?.phase !== "checking")
      throw new Error("There is no move to stop");
    this.setMove(null);
  };

  // Online answers

  getOnline = async (): Promise<OnlineView> => this.online.view();
  checkOnline = (form: OnlineForm): Promise<CheckOutcome> => this.online.check(form);
  cancelOnlineCheck = (): Promise<void> => this.online.cancelCheck();
  turnOnOnline = (settings: OnlineSettings): Promise<OnlineView> => this.online.turnOn(settings);
  turnOffOnline = (forget: boolean): Promise<OnlineView> => this.online.turnOff(forget);
  abandonOnlineSetup = (): Promise<void> => this.online.abandonSetup();
  seeOnlineChange = (): Promise<OnlineView> => this.online.seeChange();
  updateOnlineSettings = (patch: OnlineSettingsPatch): Promise<OnlineView> =>
    this.online.update(patch);
  listOnlineSends = (limit: number, before: number | null): Promise<OnlineSend[]> =>
    this.online.sends(limit, before);

  // Choosing the model for each job

  getModels = async (): Promise<ModelsView> => this.models.view();
  listModelChoices = (job: Job): Promise<ModelChoice[]> => this.models.choices(job);
  previewModelChange = (job: Job, model: string): Promise<ChangePreview> =>
    this.models.preview(job, model);
  changeModel = (job: Job, model: string, allowDownload: boolean): Promise<ModelsView> =>
    this.models.change(job, model, allowDownload);
  pauseModelDownload = (): Promise<ModelsView> => this.models.pause();
  resumeModelDownload = (): Promise<ModelsView> => this.models.resume();
  cancelModelChange = (): Promise<ModelsView> => this.models.cancel();
  cancelReindex = (): Promise<ModelsView> => this.models.cancelReindex();
  keepOldModel = (): Promise<ModelsView> => this.models.keepOld();
  removeOldModel = (): Promise<ModelsView> => this.models.removeOld();
  resetModelsToTier = (): Promise<ModelsView> => this.models.reset();
}

export function createMockEngine(options: MockOptions = {}): MockEngine {
  return new MockEngine(options);
}

export { SAFETY_PREDICATES };
