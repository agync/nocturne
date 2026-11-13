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

  constructor() {
    for (const name of ENGINE_EVENT_NAMES) {
      this.listeners.set(name, new Set());
      this.unlisten.push(this.subscribe(name));
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

  ask: EngineClient["ask"] = (question) => this.call("ask", { question });
  cancelAsk: EngineClient["cancelAsk"] = (askId) => this.call("cancel_ask", { askId });
  appReady: EngineClient["appReady"] = () => this.call("app_ready");

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
  getSettings: EngineClient["getSettings"] = () => this.call("get_settings");
  updateSettings: EngineClient["updateSettings"] = (patch) =>
    this.call("update_settings", { patch });
  pause: EngineClient["pause"] = (until) => this.call("pause", { until });
  resume: EngineClient["resume"] = () => this.call("resume");
  getAdvanced: EngineClient["getAdvanced"] = () => this.call("get_advanced");
  speakPage: EngineClient["speakPage"] = (date) => this.call("speak_page", { date });
}
