import { localDay } from "../../copy/format";
import type {
  AdvancedView,
  AskDelta,
  EngineStatus,
  EntityKind,
  EntityView,
  ExportReport,
  ForgetPreview,
  ForgetReport,
  ItemFilter,
  ItemKind,
  ItemView,
  Kept,
  MemoryFilter,
  MemoryView,
  NeedsYouEntry,
  PageLine,
  PageSummary,
  PageView,
  PrivacyView,
  QuestionChoice,
  Receipt,
  SettingsPatch,
  SettingsView,
  SetupView,
  Source,
  Tier,
  WhichResolved,
} from "../bindings";
import type { EngineClient, EngineEventName, EngineEvents, Unsubscribe } from "../client";
import { buildSeed, DEFAULT_NOW, SAFETY_PREDICATES } from "./seed";
import { privacySeed } from "./seed/privacy";
import { advancedSeed } from "./seed/setup";

export interface MockOptions {
  /** The fixed clock. Defaults to `VITE_MOCK_NOW`, else 2026-11-13T07:02:00Z. */
  now?: Date | string;
}

/** How long after keeping something it can still be taken back. */
const UNKEEP_MINUTES = 10;

const clone = <T>(value: T): T => structuredClone(value);

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
  private readonly entities: EntityView[];
  private pages: PageView[];
  private settings: SettingsView;
  private status: EngineStatus;
  private setup: SetupView;
  private readonly listeners = new Map<EngineEventName, Set<(payload: never) => void>>();
  private readonly previews = new Map<string, { ids: string[]; includeOriginals: boolean }>();
  private counter = 0;

  constructor(options: MockOptions = {}) {
    const raw = options.now ?? (import.meta.env.VITE_MOCK_NOW || DEFAULT_NOW);
    this.clock = new Date(raw);
    const seed = buildSeed(this.clock);
    this.items = seed.items;
    this.memories = seed.memories;
    this.entities = seed.entities;
    this.pages = seed.pages;
    this.settings = seed.settings;
    this.status = {
      state: "resting",
      needsYou: 0,
      waitingToRead: 0,
      sentToInternetBytes: 0,
      sentMeasured: true,
      pausedUntil: null,
      tidy: null,
      nextTidy: "01:00",
      notices: [],
    };
    this.setup = seed.setup;
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

  private refreshStatus(emit = false): void {
    const waiting = this.items.filter((i) => i.status === "kept" || i.status === "reading").length;
    this.status = {
      ...this.status,
      needsYou: this.openNeeds(),
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
    const n = this.openNeeds();
    page.headline =
      n === 0
        ? "Nothing needs you today."
        : n === 1
          ? "One thing needs you today."
          : n === 2
            ? "Two things need you today."
            : `${n} things need you today.`;
  }

  private needsChanged(): void {
    this.retitle();
    this.refreshStatus(true);
    this.emit("needs-you-changed", this.status.needsYou);
  }

  // Status and setup

  async getStatus(): Promise<EngineStatus> {
    return clone(this.status);
  }

  async getSetup(): Promise<SetupView> {
    return clone(this.setup);
  }

  async startSetup(tier: Tier): Promise<void> {
    this.setup = { ...this.setup, phase: "downloading", chosen: tier, paused: null };
    this.emit("setup-progress", this.setup);
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
      ...extra,
    };
    this.items.unshift(item);
    this.scheduleReading(item.id, source === "pastedWeb");
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
    setTimeout(
      () =>
        change({
          status: web ? "unconfirmed" : "understood",
          understoodInMs: 3000,
          memoryCount: 1,
        }),
      4500,
    );
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

  async keepFiles(paths: string[], source: Source): Promise<Kept[]> {
    return paths.map((path) => {
      const name = path.split(/[\\/]/).pop() ?? path;
      // The same file kept twice is the same thing: say so, never add a second copy.
      const duplicate = this.items.find((i) => i.fileName === name);
      if (duplicate) return { type: "duplicate", item: clone(duplicate) } satisfies Kept;
      const item = this.newItem(kindFromName(name), name, source, { fileName: name });
      return { type: "new", item: clone(item) } satisfies Kept;
    });
  }

  async keepBytes(bytes: Uint8Array, mime: string, source: Source): Promise<Kept> {
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
    });
    return { type: "new", item: clone(item) };
  }

  async unkeep(id: string): Promise<void> {
    const found = this.items.find((i) => i.id === id);
    if (!found) throw new Error(`No item ${id}`);
    // Taking back is only for something kept a moment ago, as in the real engine.
    if (this.clock.getTime() - new Date(found.keptAt).getTime() > UNKEEP_MINUTES * 60_000) {
      throw new Error(`Item ${id} was kept too long ago to take back`);
    }
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

  async keepRecording(bytes: Uint8Array, _mime: string): Promise<Kept> {
    const item = this.newItem("audio", "Voice note", "recording", {
      durationMs: Math.max(1000, Math.round(bytes.length / 16)),
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
    const found = this.items.find((i) => i.id === id);
    if (!found) throw new Error(`No item ${id}`);
    return clone(found);
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

  async cancelAsk(askId: string): Promise<void> {
    this.cancelledAsks.add(askId);
  }

  async appReady(): Promise<void> {}

  async ask(question: string): Promise<string> {
    this.counter += 1;
    const askId = `ask-${this.counter}`;
    this.runAsk(askId, question);
    return askId;
  }

  private runAsk(askId: string, question: string): void {
    const delta = (d: Partial<AskDelta> & Pick<AskDelta, "phase">): AskDelta => ({
      askId,
      textDelta: null,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
      ...d,
    });
    const send = (d: AskDelta): void => {
      if (!this.cancelledAsks.has(askId)) this.emit("ask-delta", d);
    };
    const tokens = contentTokens(question);
    const scored = (status: MemoryView["status"]) =>
      this.memories
        .filter((m) => m.status === status && !m.private)
        .map((m) => ({ m, score: scoreMemory(tokens, m) }))
        .filter((x) => tokens.length > 0 && x.score / tokens.length >= 0.5)
        .sort((a, b) => b.score - a.score);
    const active = scored("active").slice(0, 2);
    const unconfirmed = scored("unconfirmed").slice(0, 2);

    setTimeout(() => send(delta({ phase: "searching" })), 0);
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
        const text = active.map((x) => x.m.sentence).join(" ");
        const pieces = text.split(/(?<=\s)/);
        pieces.forEach((piece, i) => {
          setTimeout(() => send(delta({ phase: "streaming", textDelta: piece })), 30 * (i + 1));
        });
        setTimeout(
          () => send(delta({ phase: "done", memoryIds: active.map((x) => x.m.id) })),
          30 * (pieces.length + 1),
        );
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
    const found = this.pages.find((p) => p.date === day);
    if (found) return clone(found);
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
      this.removeMemories([id], false);
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

  async getEntity(id: string): Promise<EntityView> {
    const found = this.entities.find((e) => e.id === id);
    if (!found) throw new Error(`No entity ${id}`);
    return clone(found);
  }

  async listEntities(kind: EntityKind | null, query: string | null): Promise<EntityView[]> {
    const q = query?.trim().toLowerCase() ?? "";
    return clone(
      this.entities
        .filter((e) => kind === null || e.kind === kind)
        .filter((e) => q === "" || `${e.name} ${e.role ?? ""}`.toLowerCase().includes(q)),
    );
  }

  async snooze(memoryId: string, _days: number): Promise<void> {
    const m = this.memory(memoryId);
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

  async previewForget(ids: string[], includeOriginals: boolean): Promise<ForgetPreview> {
    this.counter += 1;
    const token = `forget-${this.counter}`;
    this.previews.set(token, { ids, includeOriginals });
    const rows = ids.map((id) => ({ memoryId: id, sentence: this.memory(id).sentence }));
    const itemIds = new Set(
      this.memories
        .filter((m) => ids.includes(m.id))
        .flatMap((m) => m.receipts.map((r) => r.itemId)),
    );
    const originals = includeOriginals
      ? [...itemIds].flatMap((itemId) => {
          const item = this.items.find((i) => i.id === itemId);
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
    };
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
    const itemIds = new Set(
      this.memories
        .filter((m) => preview.ids.includes(m.id))
        .flatMap((m) => m.receipts.map((r) => r.itemId)),
    );
    const { pages } = this.removeMemories(preview.ids, true);
    let originals = 0;
    if (preview.includeOriginals) {
      for (const itemId of itemIds) {
        const stillUsed = this.memories.some((m) => m.receipts.some((r) => r.itemId === itemId));
        if (!stillUsed) {
          this.items = this.items.filter((i) => i.id !== itemId);
          originals += 1;
        }
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
      }
    }
    return out;
  }

  async listFading(): Promise<MemoryView[]> {
    return clone(this.memories.filter((m) => m.status === "fading" || m.status === "letGo"));
  }

  // System

  async getPrivacy(): Promise<PrivacyView> {
    return {
      ...privacySeed(this.clock),
      sentToInternetBytes: this.status.sentToInternetBytes,
      sentMeasured: this.status.sentMeasured,
      lock: this.settings.lock,
      checkUpdates: this.settings.checkUpdates,
    };
  }

  async exportTo(dir: string): Promise<ExportReport> {
    const report: ExportReport = {
      path: `${dir}/Nocturne export`,
      memories: this.memories.length,
      pages: this.pages.length,
      originals: this.items.filter((i) => i.hasOriginal).length,
    };
    const total = report.memories + report.pages + report.originals;
    for (const done of [Math.round(total / 3), Math.round((2 * total) / 3), total]) {
      setTimeout(() => this.emit("export-progress", { done, total }), done);
    }
    return report;
  }

  async deleteEverything(confirmPhrase: string): Promise<void> {
    if (confirmPhrase !== "delete everything") throw new Error("The confirmation did not match");
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

  async getSettings(): Promise<SettingsView> {
    return clone(this.settings);
  }

  async updateSettings(patch: SettingsPatch): Promise<SettingsView> {
    const next = { ...this.settings } as Record<string, unknown>;
    for (const [key, value] of Object.entries(patch)) {
      if (value !== null) next[key] = value;
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
    return advancedSeed({ memories: this.memories.length, items: this.items.length });
  }

  async speakPage(date: string): Promise<string> {
    return `mock-audio/${date}.wav`;
  }
}

export function createMockEngine(options: MockOptions = {}): MockEngine {
  return new MockEngine(options);
}

export { SAFETY_PREDICATES };
