import type {
  AdvancedView,
  AskDelta,
  DeviceView,
  Edition,
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
  MergeOutcome,
  NeedsYouEntry,
  PageSummary,
  PageView,
  PairingOffer,
  PairingRequest,
  PrivacyView,
  QuestionChoice,
  Refiled,
  RefileTarget,
  RemoteConnection,
  RemoteEvent,
  RemoteSettingsPatch,
  RemoteView,
  RenameOutcome,
  SettingsPatch,
  SettingsView,
  SetupView,
  Source,
  SpokenPage,
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
  "remote-changed": RemoteView;
  "pairing-request": PairingRequest;
  "device-active": DeviceView;
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
  "remote-changed",
  "pairing-request",
  "device-active",
];

/** What a window can do about recording: `available`, or why it cannot. */
export type RecordingState = "available" | "not-secure" | "no-devices" | "no-recorder";

/**
 * Everything the interface may ask of the engine. One async method per contract command; the
 * Tauri implementation invokes the snake_case command of the same name.
 */
export interface EngineClient {
  /** The first screen has painted: the app shows its window now. */
  appReady(): Promise<void>;
  /** Says once, in the log, whether this window can record: `available` or why not. */
  reportRecording(state: RecordingState): Promise<void>;
  getStatus(): Promise<EngineStatus>;
  getSetup(): Promise<SetupView>;
  startSetup(tier: Tier): Promise<void>;
  pauseSetup(): Promise<void>;
  resumeSetup(): Promise<void>;
  /** The smaller mind on offer because memory is short, while the offer stands. */
  getTierOffer(): Promise<Tier | null>;
  /** "Not now" to that offer. It returns only after memory runs short twice more. */
  declineTierOffer(): Promise<void>;
  /** The tier of a change the person asked for that did not finish, while its notice stands. */
  getUnfinishedTier(): Promise<Tier | null>;
  /** "Not now" to the notice about it. */
  declineUnfinishedTier(): Promise<void>;
  /** Starts the private engine afresh now, instead of waiting for the next timed attempt. */
  retryEngine(): Promise<void>;
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
  /**
   * Gives a person, place or thing a new name, everywhere. A name another already has changes
   * nothing and comes back as `exists`, so the person can be offered a merge.
   */
  renameEntity(id: string, name: string): Promise<RenameOutcome>;
  /** Makes two of them one, named as `intoId` is. Taken back with `undoChange`. */
  mergeEntities(fromId: string, intoId: string): Promise<MergeOutcome>;
  /** Moves one memory to another person, place or thing, or to a new one. */
  refileMemory(id: string, target: RefileTarget): Promise<Refiled>;
  /** Takes back a rename, a merge or a move, by the change id it returned. */
  undoChange(changeId: string): Promise<void>;
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
  /** The spoken edition of a page, made once and kept; private lines are never spoken. */
  speakPage(date: string): Promise<SpokenPage>;
  /** A saved page in another edition (audio and print leave private lines out). */
  getEdition(date: string, edition: Edition): Promise<PageView>;
  /** Opens the system's sound settings, where the microphone is allowed. */
  openSoundSettings(): Promise<void>;
  /** The command line that opens quick capture, for a desktop shortcut. */
  quickKeepCommand(): Promise<string>;

  /** Away from home: reaching Nocturne from the person's own phones. Main window only. */
  getRemote(): Promise<RemoteView>;
  /** Turns it on; `allowDownload` is the person's yes to fetching what the connection needs. */
  startRemote(connection: RemoteConnection, allowDownload: boolean): Promise<RemoteView>;
  /** Turns it off; `removeAll` also unpairs every phone. */
  stopRemote(removeAll: boolean): Promise<RemoteView>;
  pauseRemoteDownload(): Promise<RemoteView>;
  /** Opens the one offer to add a phone, cancelling any other. */
  beginPairing(): Promise<PairingOffer>;
  decidePairing(pairingId: string, allow: boolean): Promise<void>;
  cancelPairing(): Promise<void>;
  renameDevice(id: string, name: string): Promise<DeviceView>;
  removeDevice(id: string): Promise<void>;
  listRemoteEvents(limit: number): Promise<RemoteEvent[]>;
  /** A new `connection` unpairs every phone. */
  updateRemoteSettings(patch: RemoteSettingsPatch): Promise<RemoteView>;
  /** Opens the sign-in page the computer holds; the interface never names the address. */
  openRemoteSignIn(): Promise<void>;
  /** A new key for this computer; every phone has to be added again. */
  newComputerKey(): Promise<RemoteView>;

  /** Subscribes. The returned function unsubscribes at once, even before the listener is attached. */
  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe;
}
