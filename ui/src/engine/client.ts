import type {
  AdvancedView,
  AskDelta,
  AskPlace,
  BackupProgress,
  BackupReport,
  BackupsView,
  BroughtCounts,
  CameInRow,
  ChangePreview,
  CheckIn,
  CheckInInput,
  CheckOutcome,
  DeviceView,
  Edition,
  EngineStatus,
  EntityKind,
  EntityView,
  EntryBlock,
  ExportProgress,
  ExportReport,
  Feeling,
  FeelingPart,
  ForgetPreview,
  ForgetReport,
  FoundItem,
  GoodThing,
  GuideView,
  HealthSeries,
  ItemFilter,
  ItemView,
  Job,
  JournalDay,
  Kept,
  MemoryFilter,
  MemoryView,
  Mention,
  MergeOutcome,
  ModelChoice,
  ModelList,
  ModelsView,
  NeedsYouEntry,
  OnlineForm,
  OnlineSend,
  OnlineSettings,
  OnlineSettingsPatch,
  OnlineView,
  PageSummary,
  PageView,
  PairingOffer,
  PairingRequest,
  Period,
  PeriodKind,
  PeriodSummary,
  PhoneDeletePreview,
  PhoneDeleteScope,
  PhonePageOptions,
  PhoneSource,
  PhoneSourcesView,
  PrivacyView,
  QuestionChoice,
  ReadingKind,
  ReadingRow,
  Refiled,
  RefileTarget,
  ReflectionView,
  RemoteConnection,
  RemoteEvent,
  RemoteSettingsPatch,
  RemoteView,
  RenameOutcome,
  RestoreCheck,
  RestoredView,
  SameNameAnswer,
  SameNameAnswered,
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

/** The headers that keep raw bytes straight into an entry. */
export function entryHeaders(entry: EntryAt | undefined): Record<string, string> {
  return entry ? { "x-nocturne-day": entry.day, "x-nocturne-block": String(entry.block) } : {};
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
  "backup-progress": BackupProgress;
  "ask-delta": AskDelta;
  "page-ready": PageSummary;
  "needs-you-changed": number;
  "remote-changed": RemoteView;
  "pairing-request": PairingRequest;
  "device-active": DeviceView;
  "online-changed": OnlineView;
  "models-changed": ModelsView;
  "move-changed": MoveView | null;
}

/** How much room a move needs on this computer and how much there is, in bytes. */
export interface MoveRoom {
  need: number;
  free: number;
}

/** A phone's things coming to this computer, as the window shows it (`get_move`). */
export type MoveView =
  /** This computer already holds a memory: the person chooses how the phone joins it. */
  | {
      phase: "offered";
      name: string;
      items: number;
      bytes: number;
      /** The first day something was kept on the phone, `YYYY-MM-DD`. */
      since?: string | null;
      /** Not enough room yet to bring its things in: Continue waits while that is chosen. */
      room?: MoveRoom | null;
    }
  /** Agreed, and nothing has arrived yet: the phone makes the copy that travels. */
  | { phase: "preparing"; name: string }
  | { phase: "arriving"; name: string; done: number; size: number }
  /** Arrived whole and being added: it can no longer be stopped once adding begins. */
  | { phase: "adding"; name: string }
  /** `removed`: the phone was removed while they were added, so it still holds its memory. */
  | { phase: "brought"; name: string; added: BroughtCounts; removed?: boolean }
  /** A bring-in that added nothing: the phone was removed here, or stopped it there. */
  | { phase: "ended"; name: string; why: "removed" | "stopped" }
  /** The person chose this computer's memory: nothing came in. */
  | { phase: "used"; name: string }
  | { phase: "failed"; name: string; why: "noRoom"; room: MoveRoom | null };

/**
 * The person's answer to a phone that offers its memory to a computer that holds one: bring the
 * phone's things in, use this computer's memory, or not now.
 */
export type MoveChoice = "notNow" | "bring" | "useHere";

export type EngineEventName = keyof EngineEvents;

export const ENGINE_EVENT_NAMES: readonly EngineEventName[] = [
  "status-changed",
  "item-changed",
  "setup-progress",
  "export-progress",
  "backup-progress",
  "ask-delta",
  "page-ready",
  "needs-you-changed",
  "remote-changed",
  "pairing-request",
  "device-active",
  "online-changed",
  "models-changed",
  "move-changed",
];

/** What a window can do about recording: `available`, or why it cannot. */
export type RecordingState = "available" | "not-secure" | "no-devices" | "no-recorder";

/**
 * Everything the interface may ask of the engine. One async method per contract command; the
 * Tauri implementation invokes the snake_case command of the same name.
 */
/** Items a delete names besides the memories: deleted even with no memories, or kept. */
export interface ForgetItems {
  also: string[];
  /** Only `also` are deleted; every other original listed stays. */
  pick: boolean;
}

/** Where a photo or voice note goes in the journal when it is kept straight into an entry. */
export interface EntryAt {
  day: string;
  block: number;
}

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
  /** With `entry`, each photo or voice note goes straight into that day's entry, from `block` on. */
  keepFiles(paths: string[], source: Source, entry?: EntryAt): Promise<Kept[]>;
  keepRecording(bytes: Uint8Array, mime: string): Promise<Kept>;
  /**
   * Keeps a pasted or dragged image, audio or document. The bytes are the raw body. With `entry`,
   * a photo or voice note is kept straight into that day's entry.
   */
  keepBytes(bytes: Uint8Array, mime: string, source: Source, entry?: EntryAt): Promise<Kept>;
  /** Takes back something just kept, as if it had not been. */
  unkeep(id: string): Promise<void>;
  /** Skips the queue: reads this item now instead of tonight. */
  readNow(id: string): Promise<ItemView>;
  setItemPrivate(id: string, isPrivate: boolean): Promise<void>;
  setItemKeepForever(id: string, keepForever: boolean): Promise<void>;
  listItems(filter: ItemFilter): Promise<ItemView[]>;
  getItem(id: string): Promise<ItemView>;
  retryItem(id: string): Promise<ItemView>;

  /**
   * Resolves with the ask id as soon as the question is accepted; the answer streams as
   * `ask-delta`. `place` is the person's choice for this one question (`auto` when left out).
   * `help` asks for the Nocturne guide's answer ("Show the guide's answer").
   */
  ask(question: string, place?: AskPlace, help?: boolean): Promise<string>;
  /** Stops an answer in progress; no more of it arrives. Stopping a finished one is fine. */
  cancelAsk(askId: string): Promise<void>;

  getPage(date?: string): Promise<PageView>;
  listPages(): Promise<PageSummary[]>;
  answerQuestion(pageDate: string, choice: QuestionChoice): Promise<void>;
  /** Adds a line to the answer of the page's question from the journal, until the page is replaced. */
  addAnswerLine(pageDate: string, line: string): Promise<void>;
  resolveWhich(conflictId: string, memoryId: string): Promise<WhichResolved>;
  confirmMemory(id: string, confirmed: boolean): Promise<void>;
  markDone(memoryId: string): Promise<void>;

  listMemories(filter: MemoryFilter): Promise<MemoryView[]>;
  /** Kept things found by their words that no mind has read yet. */
  findUnread(query: string): Promise<FoundItem[]>;
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
  /**
   * `items.also` are deleted with the originals even when no memory came from them (a check-in);
   * with `items.pick`, only they are deleted.
   */
  previewForget(
    ids: string[],
    includeOriginals: boolean,
    items?: ForgetItems,
  ): Promise<ForgetPreview>;
  forget(previewToken: string): Promise<ForgetReport>;
  listNeedsYou(): Promise<NeedsYouEntry[]>;
  listFading(): Promise<MemoryView[]>;

  getPrivacy(): Promise<PrivacyView>;
  /** Progress arrives as `export-progress` events while it runs. */
  exportTo(dir: string): Promise<ExportReport>;
  deleteEverything(confirmPhrase: string): Promise<void>;

  /** About how many bytes a backup would take. */
  estimateBackup(): Promise<number>;
  /**
   * Writes the backup to `path`; progress arrives as `backup-progress` events. Rejects with
   * `invalid`, `busy`, `noRoom`, `cantWrite`, `notFound`, `damagedBackup` or `cancelled`.
   */
  makeBackup(path: string, passphrase: string): Promise<BackupReport>;
  /** Stops a backup, a check or a restore that is running; a finished one is unaffected. */
  cancelBackup(): Promise<void>;
  getBackups(): Promise<BackupsView>;
  /** Clears the notices about a backup or restore that stopped unexpectedly. */
  dismissBackupNotices(): Promise<void>;

  /** Whether a picked file is a Nocturne backup at all, before any passphrase is asked for. */
  probeBackup(path: string): Promise<void>;
  /** Reads the whole file and says what restoring it would do here. */
  checkBackup(path: string, passphrase: string): Promise<RestoreCheck>;
  restoreBackup(path: string, passphrase: string, replace: boolean): Promise<void>;
  /** Brings back one of the copies put aside; what is here now is put aside in turn. */
  restorePutAside(id: string): Promise<void>;
  deletePutAside(id: string): Promise<void>;
  /** What the last restore brought in, until the screen has shown it. */
  getRestored(): Promise<RestoredView | null>;
  finishRestore(): Promise<void>;
  discardRestore(): Promise<void>;
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
  /** Opens the desktop's own settings for the quick keep shortcut it holds. */
  configureShortcut(): Promise<void>;
  /** Asks the desktop again for the quick keep shortcut it did not add. */
  retryShortcut(): Promise<void>;

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
  /** `alsoDeleteBrought` also deletes the calendar and health readings it brought. */
  removeDevice(id: string, alsoDeleteBrought?: boolean): Promise<void>;
  listRemoteEvents(limit: number): Promise<RemoteEvent[]>;
  /** A new `connection` unpairs every phone. */
  updateRemoteSettings(patch: RemoteSettingsPatch): Promise<RemoteView>;
  /** Opens the sign-in page the computer holds; the interface never names the address. */
  openRemoteSignIn(): Promise<void>;
  /** A new key for this computer; every phone has to be added again. */
  newComputerKey(): Promise<RemoteView>;
  /** A phone's memory moving here, if one is; it follows `move-changed`. */
  getMove(): Promise<MoveView | null>;
  answerMove(choice: MoveChoice): Promise<void>;
  /** "Open today's page", or "OK" on a move that changed nothing. */
  finishMove(): Promise<void>;
  /** "Stop moving" while a phone's memory arrives, until the restart. */
  stopMove(): Promise<void>;

  /** Online answers. Main window only; the key goes out in `checkOnline` and never comes back. */
  getOnline(): Promise<OnlineView>;
  /** One short question to the address the person entered, through the engine. */
  checkOnline(form: OnlineForm): Promise<CheckOutcome>;
  /** One look at the service's list of models with the address and key entered. */
  listOnlineModels(form: OnlineForm): Promise<ModelList>;
  cancelOnlineCheck(): Promise<void>;
  turnOnOnline(settings: OnlineSettings): Promise<OnlineView>;
  /** Turns it off; `forget` also removes the key. */
  turnOffOnline(forget: boolean): Promise<OnlineView>;
  /** Leaving setup before saying yes: the key and the route are dropped. */
  abandonOnlineSetup(): Promise<void>;
  /** The person has seen that only what they mark private is held back. */
  seeOnlineChange(): Promise<OnlineView>;
  updateOnlineSettings(patch: OnlineSettingsPatch): Promise<OnlineView>;
  /** What was sent, newest first; `before` is the id of the oldest already shown. */
  listOnlineSends(limit: number, before: number | null): Promise<OnlineSend[]>;

  /** Choosing the model for each job. Main window only. */
  getModels(): Promise<ModelsView>;
  listModelChoices(job: Job): Promise<ModelChoice[]>;
  previewModelChange(job: Job, model: string): Promise<ChangePreview>;
  /** `allowDownload` is the person's yes to fetching the model. */
  changeModel(job: Job, model: string, allowDownload: boolean): Promise<ModelsView>;
  pauseModelDownload(): Promise<ModelsView>;
  resumeModelDownload(): Promise<ModelsView>;
  cancelModelChange(): Promise<ModelsView>;
  cancelReindex(): Promise<ModelsView>;
  keepOldModel(): Promise<ModelsView>;
  removeOldModel(): Promise<ModelsView>;
  resetModelsToTier(): Promise<ModelsView>;

  /** The Nocturne guide as it came with this version. Read from inside the app; nothing is fetched. */
  getGuide(): Promise<GuideView>;

  /** The journal (docs/design/journaling.md). Days are journal days, `YYYY-MM-DD`. */
  /** One day as the journal opens it; today when `day` is absent. */
  getJournalDay(day?: string): Promise<JournalDay>;
  /** Every day with anything in the journal, oldest first, for "Choose a day". */
  listJournalDays(): Promise<string[]>;
  /** Weeks, months or years with anything kept, newest first. */
  listPeriods(kind: PeriodKind): Promise<PeriodSummary[]>;
  /** "Got it" on the first-use card. */
  dismissJournalIntro(): Promise<void>;
  /**
   * Saves a paragraph of the day's entry as the person writes. `itemId` is the block being
   * changed, absent for a new one. A block not read yet changes in place; a read block keeps its
   * earlier version as history. An empty text removes a block not read yet and returns null.
   */
  saveEntryText(
    day: string,
    itemId: string | null,
    block: number,
    text: string,
  ): Promise<EntryBlock | null>;
  /** Places a voice note or photo already kept in the day's entry. */
  placeInEntry(itemId: string, day: string, block: number): Promise<EntryBlock>;
  /** "Describe this photo": the photo's alternative text. */
  describePhoto(itemId: string, text: string): Promise<void>;
  /** The person left the journal: the day's entry is read now rather than after a pause. */
  readEntryNow(day: string): Promise<void>;
  /**
   * Keeps a quick note from the keep bar, quick capture or the journal, with an optional
   * feeling tapped beside it (kept as a check-in about the note) and the good-thing mark.
   */
  keepNote(text: string, source: Source, feeling: Feeling | null, good: boolean): Promise<Kept>;
  /** The four words this person used most in a part, or the first four of the list. */
  topFeelingWords(part: FeelingPart): Promise<string[]>;
  keepCheckIn(input: CheckInInput): Promise<CheckIn>;
  /** "I tried this" for one of the ideas in "Something that might help", by its id. */
  markTried(checkinId: string, idea: string): Promise<void>;
  /** A good thing on `day`, with a photo already kept (a new one, or one from that day). */
  keepGoodThing(day: string, text: string, photoItemId: string | null): Promise<GoodThing>;
  /** "Leave out of reflections" and its Undo. */
  setLeftOut(itemId: string, leftOut: boolean): Promise<void>;
  /** "Move to {weekday}" for a quick note kept after the day it was about. */
  moveToDay(itemId: string, day: string): Promise<void>;
  /** "That's right", with the role typed on a new person's card. */
  confirmMention(mentionId: string, role: string | null): Promise<Mention>;
  /** "Not about {name}": the memories this item added about them are let go. */
  removeMention(mentionId: string): Promise<Mention>;
  /** Undo of "Not about {name}". */
  restoreMention(mentionId: string): Promise<Mention>;
  /** "Someone else…" and "Not a person": only the memories from this item move. */
  refileMention(mentionId: string, target: RefileTarget): Promise<Mention>;
  answerSameName(questionId: string, answer: SameNameAnswer): Promise<SameNameAnswered>;
  /** Days an entity appears in the journal, newest first ("In your journal"). */
  entityJournalDays(entityId: string): Promise<string[]>;
  /** The reflection or recap for a period: written, at a glance, or nothing yet. */
  getReflection(period: Period): Promise<ReflectionView>;
  /** Writes it now on this computer ("Reflect on today", "Write it again"). Can take a minute. */
  reflect(period: Period): Promise<ReflectionView>;
  /** "Stop" while it is being written. */
  stopReflecting(period: Period): Promise<void>;
  /** "Delete this reflection": removes only the writing. */
  deleteReflection(period: Period): Promise<void>;

  /** From your phones: what paired phone apps bring. Main window only. */
  getPhoneSources(): Promise<PhoneSourcesView>;
  /** Stops or starts taking one source from one phone; stopping deletes nothing. */
  setPhoneSource(deviceId: string, source: PhoneSource, taking: boolean): Promise<PhoneSourcesView>;
  setPhonePageOptions(options: PhonePageOptions): Promise<PhoneSourcesView>;
  /** "Notice what goes together". */
  setNoticing(on: boolean): Promise<void>;
  /**
   * "What came in", newest first. Load more passes the last row's `cameInAt` as `before` and
   * its `itemId` as `beforeId`: every row of one send came in at the same time.
   */
  listCameIn(
    deviceId: string,
    source: PhoneSource,
    before: string | null,
    limit: number,
    beforeId?: string | null,
  ): Promise<CameInRow[]>;
  /** One kind over days, `from` to `to` both included. */
  getHealthSeries(kind: ReadingKind, from: string, to: string): Promise<HealthSeries>;
  /** "Show as a list": every reading of one kind, `from` to `to` both included. */
  listReadings(kind: ReadingKind, from: string, to: string): Promise<ReadingRow[]>;
  previewPhoneDelete(scope: PhoneDeleteScope): Promise<PhoneDeletePreview>;
  /** Applies a previewed delete; `alsoStop` stops taking that source from the phone too. */
  deletePhoneData(token: string, alsoStop: boolean): Promise<void>;

  /** Subscribes. The returned function unsubscribes at once, even before the listener is attached. */
  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe;
}
