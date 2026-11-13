import type {
  AdvancedView,
  AskDelta,
  EngineStatus,
  EntityView,
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
} from "./bindings";

export type Unsubscribe = () => void;

/** Event names and payloads, as the app shell emits them (crates/contract/src/events.rs). */
export interface EngineEvents {
  "status-changed": EngineStatus;
  "item-changed": ItemView;
  "setup-progress": SetupView;
  "ask-delta": AskDelta;
  "page-ready": PageSummary;
  "needs-you-changed": number;
  "files-dropped": string[];
}

export type EngineEventName = keyof EngineEvents;

export const ENGINE_EVENT_NAMES: readonly EngineEventName[] = [
  "status-changed",
  "item-changed",
  "setup-progress",
  "ask-delta",
  "page-ready",
  "needs-you-changed",
  "files-dropped",
];

/**
 * Everything the interface may ask of the engine. One async method per contract command; the
 * Tauri implementation invokes the snake_case command of the same name.
 */
export interface EngineClient {
  getStatus(): Promise<EngineStatus>;
  getSetup(): Promise<SetupView>;
  startSetup(tier: Tier): Promise<void>;
  pauseSetup(): Promise<void>;
  resumeSetup(): Promise<void>;

  keepText(text: string, source: Source): Promise<Kept>;
  keepFiles(paths: string[], source: Source): Promise<Kept[]>;
  keepRecording(bytes: number[], mime: string): Promise<Kept>;
  listItems(filter: ItemFilter): Promise<ItemView[]>;
  getItem(id: string): Promise<ItemView>;
  retryItem(id: string): Promise<ItemView>;

  /** Resolves with the ask id as soon as the question is accepted; the answer streams as `ask-delta`. */
  ask(question: string): Promise<string>;

  getPage(date?: string): Promise<PageView>;
  listPages(): Promise<PageSummary[]>;
  answerQuestion(pageDate: string, choice: QuestionChoice): Promise<void>;
  resolveWhich(conflictId: string, memoryId: string): Promise<void>;
  confirmMemory(id: string, confirmed: boolean): Promise<void>;
  markDone(memoryId: string): Promise<void>;

  listMemories(filter: MemoryFilter): Promise<MemoryView[]>;
  getEntity(id: string): Promise<EntityView>;
  correctMemory(id: string, text: string): Promise<MemoryView>;
  setKeepForever(id: string, keepForever: boolean): Promise<void>;
  setPrivate(id: string, isPrivate: boolean): Promise<void>;
  restore(ids: string[]): Promise<void>;
  previewForget(ids: string[], includeOriginals: boolean): Promise<ForgetPreview>;
  forget(previewToken: string): Promise<ForgetReport>;
  listNeedsYou(): Promise<NeedsYouEntry[]>;
  listFading(): Promise<MemoryView[]>;

  getPrivacy(): Promise<PrivacyView>;
  exportTo(dir: string): Promise<void>;
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
