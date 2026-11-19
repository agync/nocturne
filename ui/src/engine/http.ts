import {
  ENGINE_EVENT_NAMES,
  type EngineClient,
  EngineError,
  type EngineEventName,
  type EngineEvents,
  entryHeaders,
  toEngineError,
  type Unsubscribe,
} from "./client";
import { authHeaders } from "./httpAuth";
import { mimeOf, takeFile } from "./previewFiles";

type Listener = (payload: never) => void;

export interface HttpEngineOptions {
  fetch?: typeof fetch;
  EventSource?: typeof EventSource;
  /** Where the server is; the page's own origin when empty. */
  base?: string;
  /** The bearer token, if there is one. */
  token?: () => string | null;
  /** Wait before the first try and between tries to hear events again, in milliseconds. */
  retryMs?: (attempt: number) => number;
}

const RETRY_CAP_MS = 10_000;

/**
 * Talks to the real engine behind the development preview server: each command is a POST with
 * the command's own camelCase arguments, and events arrive over one server-sent stream that
 * is reopened, with a fresh ticket, whenever it drops. Only for development, never shipped.
 */
export class HttpEngine implements EngineClient {
  private readonly listeners = new Map<EngineEventName, Set<Listener>>();
  private readonly fetcher: typeof fetch;
  private readonly Source: typeof EventSource;
  private readonly base: string;
  private readonly token: () => string | null;
  private readonly retryMs: (attempt: number) => number;
  private source: EventSource | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private attempt = 0;
  private disposed = false;

  constructor(options: HttpEngineOptions = {}) {
    this.fetcher = options.fetch ?? ((...args) => globalThis.fetch(...args));
    this.Source = options.EventSource ?? globalThis.EventSource;
    this.base = options.base ?? "";
    this.token = options.token ?? (() => null);
    this.retryMs = options.retryMs ?? ((n) => Math.min(RETRY_CAP_MS, 500 * 2 ** n));
    for (const name of ENGINE_EVENT_NAMES) this.listeners.set(name, new Set());
    void this.connect();
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.wake);
    }
  }

  /** A phone that slept may have lost the stream without saying so. */
  private readonly wake = () => {
    if (this.disposed || document.visibilityState !== "visible") return;
    if (!this.source || this.source.readyState === 2) void this.connect();
  };

  private async connect(): Promise<void> {
    if (this.disposed) return;
    this.close();
    try {
      const { ticket } = await this.send<{ ticket: string }>("/api/ticket", {
        kind: "events",
      });
      if (this.disposed) return;
      const source = new this.Source(
        `${this.base}/api/events?ticket=${encodeURIComponent(ticket)}`,
      );
      this.source = source;
      for (const name of ENGINE_EVENT_NAMES) {
        source.addEventListener(name, (event) => {
          let payload: unknown;
          try {
            payload = JSON.parse((event as MessageEvent<string>).data);
          } catch {
            return;
          }
          for (const cb of this.listeners.get(name) ?? []) cb(payload as never);
        });
      }
      source.addEventListener("open", () => {
        this.attempt = 0;
      });
      // A refused or dropped stream is closed and tried again with a new ticket: the browser
      // would retry the old one, which is spent.
      source.addEventListener("error", () => this.retry());
    } catch {
      this.retry();
    }
  }

  private retry(): void {
    if (this.disposed || this.timer) return;
    this.close();
    const wait = this.retryMs(this.attempt);
    this.attempt += 1;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.connect();
    }, wait);
  }

  private close(): void {
    this.source?.close();
    this.source = null;
  }

  dispose(): void {
    this.disposed = true;
    this.close();
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", this.wake);
    }
  }

  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe {
    const set = this.listeners.get(name);
    set?.add(cb as Listener);
    return () => {
      set?.delete(cb as Listener);
    };
  }

  /** One request; a failure of any kind is an `EngineError`. */
  private async request<T>(path: string, init: RequestInit): Promise<T> {
    let response: Response;
    try {
      response = await this.fetcher(`${this.base}${path}`, {
        ...init,
        headers: { ...authHeaders(this.token()), ...(init.headers as Record<string, string>) },
      });
    } catch (error) {
      throw new EngineError("unavailable", error instanceof Error ? error.message : String(error));
    }
    let payload: unknown = null;
    try {
      payload = await response.json();
    } catch {
      payload = null;
    }
    if (!response.ok) {
      if (payload && typeof payload === "object") throw toEngineError(payload);
      throw new EngineError(
        response.status === 401 ? "unauthorized" : "unknown",
        response.statusText,
      );
    }
    return payload as T;
  }

  private send<T>(path: string, args?: Record<string, unknown>): Promise<T> {
    return this.request<T>(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
  }

  private call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return this.send<T>(`/api/invoke/${command}`, args);
  }

  private raw<T>(command: string, bytes: Uint8Array, headers: Record<string, string>): Promise<T> {
    return this.request<T>(`/api/invoke/${command}`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream", ...headers },
      body: bytes as unknown as BodyInit,
    });
  }

  getStatus = () => this.call<Awaited<ReturnType<EngineClient["getStatus"]>>>("get_status");
  getSetup = () => this.call<Awaited<ReturnType<EngineClient["getSetup"]>>>("get_setup");
  startSetup: EngineClient["startSetup"] = (tier) => this.call("start_setup", { tier });
  pauseSetup: EngineClient["pauseSetup"] = () => this.call("pause_setup");
  resumeSetup: EngineClient["resumeSetup"] = () => this.call("resume_setup");
  getTierOffer: EngineClient["getTierOffer"] = () => this.call("get_tier_offer");
  declineTierOffer: EngineClient["declineTierOffer"] = () => this.call("decline_tier_offer");
  getUnfinishedTier: EngineClient["getUnfinishedTier"] = () => this.call("get_unfinished_tier");
  declineUnfinishedTier: EngineClient["declineUnfinishedTier"] = () =>
    this.call("decline_unfinished_tier");
  retryEngine: EngineClient["retryEngine"] = () => this.call("retry_engine");
  tidyAnyway: EngineClient["tidyAnyway"] = () => this.call("tidy_anyway");

  keepText: EngineClient["keepText"] = (text, source) => this.call("keep_text", { text, source });
  /**
   * A browser has no paths to give: what the platform handed out are keys of files the person
   * chose, and each one travels as bytes.
   */
  keepFiles: EngineClient["keepFiles"] = async (keys, source, entry) => {
    const kept = [];
    for (const [n, key] of keys.entries()) {
      const file = takeFile(key);
      if (!file) throw new EngineError("invalid", "that file was not chosen");
      const bytes = new Uint8Array(await file.arrayBuffer());
      const at = entry ? { day: entry.day, block: entry.block + n } : undefined;
      kept.push(await this.keepBytes(bytes, mimeOf(file), source, at));
    }
    return kept;
  };
  /** The audio travels as the raw request body; the mime type rides in a header. */
  keepRecording: EngineClient["keepRecording"] = (bytes, mime) =>
    this.raw("keep_recording", bytes, { "x-nocturne-mime": mime });
  keepBytes: EngineClient["keepBytes"] = (bytes, mime, source, entry) =>
    this.raw("keep_bytes", bytes, {
      "x-nocturne-mime": mime,
      "x-nocturne-source": source,
      ...entryHeaders(entry),
    });
  unkeep: EngineClient["unkeep"] = (id) => this.call("unkeep", { id });
  readNow: EngineClient["readNow"] = (id) => this.call("read_now", { id });
  setItemPrivate: EngineClient["setItemPrivate"] = (id, isPrivate) =>
    this.call("set_item_private", { id, private: isPrivate });
  setItemKeepForever: EngineClient["setItemKeepForever"] = (id, keepForever) =>
    this.call("set_item_keep_forever", { id, keepForever });
  listItems: EngineClient["listItems"] = (filter) => this.call("list_items", { filter });
  getItem: EngineClient["getItem"] = (id) => this.call("get_item", { id });
  retryItem: EngineClient["retryItem"] = (id) => this.call("retry_item", { id });

  ask: EngineClient["ask"] = (question, place, help) =>
    this.call("ask", { question, place: place ?? null, help: help ?? null });
  cancelAsk: EngineClient["cancelAsk"] = (askId) => this.call("cancel_ask", { askId });
  appReady: EngineClient["appReady"] = () => this.call("app_ready");
  reportRecording: EngineClient["reportRecording"] = (state) =>
    this.call("report_recording", { state });

  getPage: EngineClient["getPage"] = (date) => this.call("get_page", { date: date ?? null });
  listPages: EngineClient["listPages"] = () => this.call("list_pages");
  answerQuestion: EngineClient["answerQuestion"] = (pageDate, choice) =>
    this.call("answer_question", { pageDate, choice });
  resolveWhich: EngineClient["resolveWhich"] = (conflictId, memoryId) =>
    this.call("resolve_which", { conflictId, memoryId });
  confirmMemory: EngineClient["confirmMemory"] = (id, confirmed) =>
    this.call("confirm_memory", { id, confirmed });
  markDone: EngineClient["markDone"] = (memoryId) => this.call("mark_done", { memoryId });

  listMemories: EngineClient["listMemories"] = (filter) => this.call("list_memories", { filter });
  getEntity: EngineClient["getEntity"] = (id) => this.call("get_entity", { id });
  listEntities: EngineClient["listEntities"] = (kind, query) =>
    this.call("list_entities", { kind, query });
  snooze: EngineClient["snooze"] = (memoryId, days) => this.call("snooze", { memoryId, days });
  correctMemory: EngineClient["correctMemory"] = (id, text) =>
    this.call("correct_memory", { id, text });
  renameEntity: EngineClient["renameEntity"] = (id, name) =>
    this.call("rename_entity", { id, name });
  mergeEntities: EngineClient["mergeEntities"] = (fromId, intoId) =>
    this.call("merge_entities", { fromId, intoId });
  refileMemory: EngineClient["refileMemory"] = (id, target) =>
    this.call("refile_memory", { id, target });
  undoChange: EngineClient["undoChange"] = (changeId) => this.call("undo_change", { changeId });
  setKeepForever: EngineClient["setKeepForever"] = (id, keepForever) =>
    this.call("set_keep_forever", { id, keepForever });
  setPrivate: EngineClient["setPrivate"] = (id, isPrivate) =>
    this.call("set_private", { id, private: isPrivate });
  restore: EngineClient["restore"] = (ids) => this.call("restore", { ids });
  previewForget: EngineClient["previewForget"] = (ids, includeOriginals, items) =>
    this.call("preview_forget", {
      ids,
      includeOriginals,
      also: items?.also,
      pick: items?.pick,
    });
  forget: EngineClient["forget"] = (previewToken) => this.call("forget", { previewToken });
  listNeedsYou: EngineClient["listNeedsYou"] = () => this.call("list_needs_you");
  listFading: EngineClient["listFading"] = () => this.call("list_fading");

  getPrivacy: EngineClient["getPrivacy"] = () => this.call("get_privacy");
  exportTo: EngineClient["exportTo"] = (dir) => this.call("export_to", { dir });
  deleteEverything: EngineClient["deleteEverything"] = (confirmPhrase) =>
    this.call("delete_everything", { confirmPhrase });

  // The server answers unavailable: backups need the system dialogs and the desktop's files.
  estimateBackup: EngineClient["estimateBackup"] = () => this.call("estimate_backup");
  makeBackup: EngineClient["makeBackup"] = (path, passphrase) =>
    this.call("make_backup", { path, passphrase });
  cancelBackup: EngineClient["cancelBackup"] = () => this.call("cancel_backup");
  getBackups: EngineClient["getBackups"] = () => this.call("get_backups");
  dismissBackupNotices: EngineClient["dismissBackupNotices"] = () =>
    this.call("dismiss_backup_notices");
  probeBackup: EngineClient["probeBackup"] = (path) => this.call("probe_backup", { path });
  checkBackup: EngineClient["checkBackup"] = (path, passphrase) =>
    this.call("check_backup", { path, passphrase });
  restoreBackup: EngineClient["restoreBackup"] = (path, passphrase, replace) =>
    this.call("restore_backup", { path, passphrase, replace });
  restorePutAside: EngineClient["restorePutAside"] = (id) => this.call("restore_put_aside", { id });
  deletePutAside: EngineClient["deletePutAside"] = (id) => this.call("delete_put_aside", { id });
  getRestored: EngineClient["getRestored"] = () => this.call("get_restored");
  finishRestore: EngineClient["finishRestore"] = () => this.call("finish_restore");
  discardRestore: EngineClient["discardRestore"] = () => this.call("discard_restore");
  getSettings: EngineClient["getSettings"] = () => this.call("get_settings");
  updateSettings: EngineClient["updateSettings"] = (patch) =>
    this.call("update_settings", { patch });
  pause: EngineClient["pause"] = (until) => this.call("pause", { until });
  resume: EngineClient["resume"] = () => this.call("resume");
  getAdvanced: EngineClient["getAdvanced"] = () => this.call("get_advanced");
  speakPage: EngineClient["speakPage"] = (date) => this.call("speak_page", { date });
  getEdition: EngineClient["getEdition"] = (date, edition) =>
    this.call("get_edition", { date, edition });
  openSoundSettings: EngineClient["openSoundSettings"] = () => this.call("open_sound_settings");
  quickKeepCommand: EngineClient["quickKeepCommand"] = () => this.call("quick_keep_command");
  configureShortcut: EngineClient["configureShortcut"] = () => this.call("configure_shortcut");
  retryShortcut: EngineClient["retryShortcut"] = () => this.call("retry_shortcut");

  getRemote: EngineClient["getRemote"] = () => this.call("get_remote");
  startRemote: EngineClient["startRemote"] = (connection, allowDownload) =>
    this.call("start_remote", { connection, allowDownload });
  stopRemote: EngineClient["stopRemote"] = (removeAll) => this.call("stop_remote", { removeAll });
  pauseRemoteDownload: EngineClient["pauseRemoteDownload"] = () =>
    this.call("pause_remote_download");
  beginPairing: EngineClient["beginPairing"] = () => this.call("begin_pairing");
  decidePairing: EngineClient["decidePairing"] = (pairingId, allow) =>
    this.call("decide_pairing", { pairingId, allow });
  cancelPairing: EngineClient["cancelPairing"] = () => this.call("cancel_pairing");
  renameDevice: EngineClient["renameDevice"] = (id, name) =>
    this.call("rename_device", { id, name });
  removeDevice: EngineClient["removeDevice"] = (id) => this.call("remove_device", { id });
  listRemoteEvents: EngineClient["listRemoteEvents"] = (limit) =>
    this.call("list_remote_events", { limit });
  updateRemoteSettings: EngineClient["updateRemoteSettings"] = (patch) =>
    this.call("update_remote_settings", { patch });
  openRemoteSignIn: EngineClient["openRemoteSignIn"] = () => this.call("open_remote_sign_in");
  newComputerKey: EngineClient["newComputerKey"] = () => this.call("new_computer_key");

  getOnline: EngineClient["getOnline"] = () => this.call("get_online");
  checkOnline: EngineClient["checkOnline"] = (form) => this.call("check_online", { form });
  cancelOnlineCheck: EngineClient["cancelOnlineCheck"] = () => this.call("cancel_online_check");
  turnOnOnline: EngineClient["turnOnOnline"] = (settings) =>
    this.call("turn_on_online", { settings });
  turnOffOnline: EngineClient["turnOffOnline"] = (forget) =>
    this.call("turn_off_online", { forget });
  abandonOnlineSetup: EngineClient["abandonOnlineSetup"] = () => this.call("abandon_online_setup");
  updateOnlineSettings: EngineClient["updateOnlineSettings"] = (patch) =>
    this.call("update_online_settings", { patch });
  listOnlineSends: EngineClient["listOnlineSends"] = (limit, before) =>
    this.call("list_online_sends", { limit, before });

  getModels: EngineClient["getModels"] = () => this.call("get_models");
  listModelChoices: EngineClient["listModelChoices"] = (job) =>
    this.call("list_model_choices", { job });
  previewModelChange: EngineClient["previewModelChange"] = (job, model) =>
    this.call("preview_model_change", { job, model });
  changeModel: EngineClient["changeModel"] = (job, model, allowDownload) =>
    this.call("change_model", { job, model, allowDownload });
  pauseModelDownload: EngineClient["pauseModelDownload"] = () => this.call("pause_model_download");
  resumeModelDownload: EngineClient["resumeModelDownload"] = () =>
    this.call("resume_model_download");
  cancelModelChange: EngineClient["cancelModelChange"] = () => this.call("cancel_model_change");
  cancelReindex: EngineClient["cancelReindex"] = () => this.call("cancel_reindex");
  keepOldModel: EngineClient["keepOldModel"] = () => this.call("keep_old_model");
  removeOldModel: EngineClient["removeOldModel"] = () => this.call("remove_old_model");
  resetModelsToTier: EngineClient["resetModelsToTier"] = () => this.call("reset_models_to_tier");
  getGuide: EngineClient["getGuide"] = () => this.call("get_guide");

  getJournalDay: EngineClient["getJournalDay"] = (day) =>
    this.call("get_journal_day", { day: day ?? null });
  listJournalDays: EngineClient["listJournalDays"] = () => this.call("list_journal_days");
  listPeriods: EngineClient["listPeriods"] = (kind) => this.call("list_periods", { kind });
  dismissJournalIntro: EngineClient["dismissJournalIntro"] = () =>
    this.call("dismiss_journal_intro");
  saveEntryText: EngineClient["saveEntryText"] = (day, itemId, block, text) =>
    this.call("save_entry_text", { day, itemId, block, text });
  placeInEntry: EngineClient["placeInEntry"] = (itemId, day, block) =>
    this.call("place_in_entry", { itemId, day, block });
  describePhoto: EngineClient["describePhoto"] = (itemId, text) =>
    this.call("describe_photo", { itemId, text });
  readEntryNow: EngineClient["readEntryNow"] = (day) => this.call("read_entry_now", { day });
  keepNote: EngineClient["keepNote"] = (text, source, feeling, good) =>
    this.call("keep_note", { text, source, feeling, good });
  topFeelingWords: EngineClient["topFeelingWords"] = (part) =>
    this.call("top_feeling_words", { part });
  keepCheckIn: EngineClient["keepCheckIn"] = (input) => this.call("keep_check_in", { input });
  markTried: EngineClient["markTried"] = (checkinId, idea) =>
    this.call("mark_tried", { checkinId, idea });
  keepGoodThing: EngineClient["keepGoodThing"] = (day, text, photoItemId) =>
    this.call("keep_good_thing", { day, text, photoItemId });
  setLeftOut: EngineClient["setLeftOut"] = (itemId, leftOut) =>
    this.call("set_left_out", { itemId, leftOut });
  moveToDay: EngineClient["moveToDay"] = (itemId, day) => this.call("move_to_day", { itemId, day });
  confirmMention: EngineClient["confirmMention"] = (mentionId, role) =>
    this.call("confirm_mention", { mentionId, role });
  removeMention: EngineClient["removeMention"] = (mentionId) =>
    this.call("remove_mention", { mentionId });
  restoreMention: EngineClient["restoreMention"] = (mentionId) =>
    this.call("restore_mention", { mentionId });
  refileMention: EngineClient["refileMention"] = (mentionId, target) =>
    this.call("refile_mention", { mentionId, target });
  answerSameName: EngineClient["answerSameName"] = (questionId, answer) =>
    this.call("answer_same_name", { questionId, answer });
  entityJournalDays: EngineClient["entityJournalDays"] = (entityId) =>
    this.call("entity_journal_days", { entityId });
  getReflection: EngineClient["getReflection"] = (period) =>
    this.call("get_reflection", { period });
  reflect: EngineClient["reflect"] = (period) => this.call("reflect", { period });
  stopReflecting: EngineClient["stopReflecting"] = (period) =>
    this.call("stop_reflecting", { period });
  deleteReflection: EngineClient["deleteReflection"] = (period) =>
    this.call("delete_reflection", { period });
}
