import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  ENGINE_EVENT_NAMES,
  type EngineClient,
  type EngineEventName,
  type EngineEvents,
  toEngineError,
  type Unsubscribe,
} from "./client";

type Listener = (payload: never) => void;

/**
 * Talks to the Rust engine through Tauri `invoke` (snake_case command, camelCase argument keys)
 * and fans events out to local listeners. It subscribes to every event once, at construction, so
 * no event is lost while a screen attaches.
 */
export class TauriEngine implements EngineClient {
  private readonly listeners = new Map<EngineEventName, Set<Listener>>();
  private readonly unlisten: Array<Promise<UnlistenFn>> = [];

  /**
   * `listen: false` for a window that is not allowed to hear events (quick capture: answers and
   * kept items travel on them). It then never asks, so nothing is refused or logged.
   */
  constructor(options: { listen?: boolean } = {}) {
    for (const name of ENGINE_EVENT_NAMES) {
      this.listeners.set(name, new Set());
      if (options.listen !== false) this.unlisten.push(this.subscribe(name));
    }
  }

  /** One attempt, then one retry; a failure is logged, never thrown into the page. */
  private subscribe(name: EngineEventName, retry = true): Promise<UnlistenFn> {
    const attach = () =>
      listen<EngineEvents[typeof name]>(name, (event) => {
        for (const cb of this.listeners.get(name) ?? []) cb(event.payload as never);
      });
    return attach().catch((error) => {
      console.error(`Could not listen for ${name}`, error);
      if (!retry) return () => undefined;
      return new Promise<UnlistenFn>((resolve) => {
        setTimeout(() => resolve(this.subscribe(name, false)), 500);
      });
    });
  }

  dispose(): void {
    for (const p of this.unlisten) void p.then((fn) => fn()).catch(() => undefined);
    this.unlisten.length = 0;
  }

  on<E extends EngineEventName>(name: E, cb: (payload: EngineEvents[E]) => void): Unsubscribe {
    const set = this.listeners.get(name);
    set?.add(cb as Listener);
    return () => {
      set?.delete(cb as Listener);
    };
  }

  private async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw toEngineError(error);
    }
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
  keepFiles: EngineClient["keepFiles"] = (paths, source) =>
    this.call("keep_files", { paths, source });
  /** The audio travels as the raw request body; the mime type rides in a header. */
  keepRecording: EngineClient["keepRecording"] = async (bytes, mime) => {
    try {
      return await invoke("keep_recording", bytes, { headers: { "x-nocturne-mime": mime } });
    } catch (error) {
      throw toEngineError(error);
    }
  };
  keepBytes: EngineClient["keepBytes"] = async (bytes, mime, source) => {
    try {
      return await invoke("keep_bytes", bytes, {
        headers: { "x-nocturne-mime": mime, "x-nocturne-source": source },
      });
    } catch (error) {
      throw toEngineError(error);
    }
  };
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
  previewForget: EngineClient["previewForget"] = (ids, includeOriginals) =>
    this.call("preview_forget", { ids, includeOriginals });
  forget: EngineClient["forget"] = (previewToken) => this.call("forget", { previewToken });
  listNeedsYou: EngineClient["listNeedsYou"] = () => this.call("list_needs_you");
  listFading: EngineClient["listFading"] = () => this.call("list_fading");

  getPrivacy: EngineClient["getPrivacy"] = () => this.call("get_privacy");
  exportTo: EngineClient["exportTo"] = (dir) => this.call("export_to", { dir });
  deleteEverything: EngineClient["deleteEverything"] = (confirmPhrase) =>
    this.call("delete_everything", { confirmPhrase });

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
}
