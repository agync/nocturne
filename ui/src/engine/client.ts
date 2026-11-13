import type {
  AdvancedView,
  AskDelta,
  EngineStatus,
  EntityKind,
  EntityView,
  ExportProgress,
  ExportReport,
  ForgetPreview,
  ForgetReport,
  ItemFilter,
  ItemView,
  Kept,
  MemoryFilter,
  MemoryView,
  NeedsYouEntry,
  PageSummary,
  PageView,
  PrivacyView,
  QuestionChoice,
  SettingsPatch,
  SettingsView,
  SetupView,
  Source,
  Tier,
  WhichResolved,
} from "./bindings";

export type Unsubscribe = () => void;

/** Every failed command reaches the interface as one of these, whatever the transport said. */
export class EngineError extends Error {
  constructor(
    readonly code: string,
    readonly detail: string,
  ) {
    super(`${code}: ${detail}`);
    this.name = "EngineError";
  }
}

/** Tauri rejects with a string or an object; this makes both an `EngineError`. */
export function toEngineError(raw: unknown): EngineError {
  if (raw instanceof EngineError) return raw;
  if (typeof raw === "string") return new EngineError("unknown", raw);
  if (raw && typeof raw === "object") {
    const o = raw as Record<string, unknown>;
    const code = typeof o.code === "string" ? o.code : "unknown";
    const detail = [o.detail, o.message, o.error].find((v) => typeof v === "string");
    return new EngineError(code, (detail as string | undefined) ?? JSON.stringify(raw));
  }
  return new EngineError("unknown", String(raw));
}

/** Event names and payloads, as the app shell emits them (crates/contract/src/events.rs). */
export interface EngineEvents {
  "status-changed": EngineStatus;
  "item-changed": ItemView;
  "setup-progress": SetupView;
  "export-progress": ExportProgress;
  "ask-delta": AskDelta;
  "page-ready": PageSummary;
  "needs-you-changed": number;
}

export type EngineEventName = keyof EngineEvents;

export const ENGINE_EVENT_NAMES: readonly EngineEventName[] = [
  "status-changed",
  "item-changed",
  "setup-progress",
  "export-progress",
  "ask-delta",
  "page-ready",
  "needs-you-changed",
];

/**
 * Everything the interface may ask of the engine. One async method per contract command; the
 * Tauri implementation invokes the snake_case command of the same name.
 */
export interface EngineClient {
  /** The first screen has painted: the app shows its window now. */
  appReady(): Promise<void>;
  getStatus(): Promise<EngineStatus>;
  getSetup(): Promise<SetupView>;
  startSetup(tier: Tier): Promise<void>;
  pauseSetup(): Promise<void>;
  resumeSetup(): Promise<void>;
  /** The smaller mind on offer because memory is short, while the offer stands. */
  getTierOffer(): Promise<Tier | null>;
  /** "Not now" to that offer. It returns only after memory runs short twice more. */
  declineTierOffer(): Promise<void>;
  /** "Tidy anyway": the battery rule is set aside until this tidy is done. */
  tidyAnyway(): Promise<void>;

  keepText(text: string, source: Source): Promise<Kept>;
  keepFiles(paths: string[], source: Source): Promise<Kept[]>;
  keepRecording(bytes: Uint8Array, mime: string): Promise<Kept>;
  /** Keeps a pasted or dragged image, audio or document. The bytes are the raw body. */
  keepBytes(bytes: Uint8Array, mime: string, source: Source): Promise<Kept>;
  /** Takes back something just kept, as if it had not been. */
  unkeep(id: string): Promise<void>;
  /** Skips the queue: reads this item now instead of tonight. */
  readNow(id: string): Promise<ItemView>;
  setItemPrivate(id: string, isPrivate: boolean): Promise<void>;
  setItemKeepForever(id: string, keepForever: boolean): Promise<void>;
  listItems(filter: ItemFilter): Promise<ItemView[]>;
  getItem(id: string): Promise<ItemView>;
  retryItem(id: string): Promise<ItemView>;

  /** Resolves with the ask id as soon as the question is accepted; the answer streams as `ask-delta`. */
  ask(question: string): Promise<string>;
  /** Stops an answer in progress; no more of it arrives. Stopping a finished one is fine. */
  cancelAsk(askId: string): Promise<void>;

  getPage(date?: string): Promise<PageView>;
  listPages(): Promise<PageSummary[]>;
  answerQuestion(pageDate: string, choice: QuestionChoice): Promise<void>;
  resolveWhich(conflictId: string, memoryId: string): Promise<WhichResolved>;
  confirmMemory(id: string, confirmed: boolean): Promise<void>;
  markDone(memoryId: string): Promise<void>;

  listMemories(filter: MemoryFilter): Promise<MemoryView[]>;
  getEntity(id: string): Promise<EntityView>;
  listEntities(kind: EntityKind | null, query: string | null): Promise<EntityView[]>;
  /** Stops a fading memory from fading for this many days. */
  snooze(memoryId: string, days: number): Promise<void>;
  correctMemory(id: string, text: string): Promise<MemoryView>;
  setKeepForever(id: string, keepForever: boolean): Promise<void>;
  setPrivate(id: string, isPrivate: boolean): Promise<void>;
  restore(ids: string[]): Promise<void>;
  previewForget(ids: string[], includeOriginals: boolean): Promise<ForgetPreview>;
  forget(previewToken: string): Promise<ForgetReport>;
  listNeedsYou(): Promise<NeedsYouEntry[]>;
  listFading(): Promise<MemoryView[]>;

  getPrivacy(): Promise<PrivacyView>;
  /** Progress arrives as `export-progress` events while it runs. */
  exportTo(dir: string): Promise<ExportReport>;
  deleteEverything(confirmPhrase: string): Promise<void>;
  getSettings(): Promise<SettingsView>;
  updateSettings(patch: SettingsPatch): Promise<SettingsView>;
  pause(until: string | null): Promise<void>;
  resume(): Promise<void>;
  getAdvanced(): Promise<AdvancedView>;
  speakPage(date: string): Promise<string>;

  /** Subscribes. The returned function unsubscribes at once, even before the listener is attached. */
  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe;
}
