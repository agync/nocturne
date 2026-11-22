// What a phone alone asks of its own memory beyond the shared screen calls: where the memory
// lives, starting, finding by words, backups and what was put aside, deleting and how much room
// is left. Each goes to the app's Rust side; backups report progress as `backup-progress` events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  BackupProgress,
  BackupReport,
  ForgetPreview,
  ForgetReport,
  FoundItem,
  HomeAnswer,
  HomeCall,
  HomeError,
  HomeStats,
  ItemKept,
  KeptCounts,
  KeptHow,
  PhoneDeletePreview,
  PhoneItem,
  PhoneMode,
  PhoneSource,
  PutAsideMemory,
  RestoreCheck,
} from "../../engine/bindings";
import { idbNotes, type NoteStore } from "./reminder";

/** A home call the phone answered with an error. */
export class HomeRequestError extends Error {
  readonly code: HomeError;
  constructor(code: HomeError) {
    super(code);
    this.name = "HomeRequestError";
    this.code = code;
  }
}

/** A backup file the person chose, copied where the app can read it. */
export interface PickedFile {
  path: string;
  name: string;
  bytes: number;
}

/** What the phone's open sheet answers once a file is chosen and found to be a backup. */
interface ChosenBackup {
  file: string;
  name: string;
  size: number;
}

/** A backup checked, with what it and this phone hold in the replace columns. */
export interface Checked {
  check: RestoreCheck;
  backup: KeptCounts;
  here: KeptCounts | null;
}

export type Progress = (p: BackupProgress) => void;

/** Whether the phone's speech reader may run, as the speech plugin says. */
export type VoiceAccess = "granted" | "denied" | "prompt" | "prompt-with-rationale";

export interface HomeApi {
  /** Where the memory lives; null before the person has chosen. */
  mode(): Promise<PhoneMode | null>;
  /** The last mode heard, without asking. */
  known(): PhoneMode | null;
  /** Called with each new mode, after a start, a restore or Delete everything. */
  onMode(listener: (mode: PhoneMode | null) => void): () => void;
  /** Set when the memory could not be opened and was put aside. */
  damaged(): Promise<boolean>;
  startAlone(name: string): Promise<void>;
  find(query: string): Promise<FoundItem[]>;
  stats(): Promise<HomeStats>;
  /** Makes the backup, then the phone's save sheet; null when the sheet was closed. */
  saveBackupFile(passphrase: string, onProgress: Progress): Promise<BackupReport | null>;
  /** The phone's open sheet; null when it was closed. */
  chooseBackupFile(): Promise<PickedFile | null>;
  /** The chosen file's copy goes, when the restore is left. */
  letGoBackupFile(): Promise<void>;
  checkBackup(file: string, passphrase: string, onProgress: Progress): Promise<Checked>;
  restoreBackup(
    file: string,
    passphrase: string,
    replace: boolean,
    onProgress: Progress,
  ): Promise<void>;
  putAside(): Promise<PutAsideMemory[]>;
  bringBack(id: string): Promise<void>;
  deletePutAside(id: string): Promise<void>;
  previewSourceDelete(source: PhoneSource): Promise<PhoneDeletePreview>;
  deleteSource(token: string, source: PhoneSource, alsoStop: boolean): Promise<void>;
  /** Notes how a thing was just kept, for its page. */
  setKeptHow(id: string, how: KeptHow): Promise<void>;
  itemKept(id: string): Promise<ItemKept>;
  /** "Forget…" (what was learned) or "Delete…" (the thing too) from an item's page. */
  previewForgetItem(id: string, withOriginal: boolean): Promise<ForgetPreview>;
  forget(token: string): Promise<ForgetReport>;
  deleteEverything(phrase: string): Promise<void>;
  /** The bytes free where the memory is; null where the phone does not say. */
  freeSpace(): Promise<number | null>;
  /** The phone's language, so waiting voice notes can be read into words where it can. */
  hearVoiceNotes(language: string): Promise<boolean>;
  voiceAccess(): Promise<VoiceAccess>;
  /** The phone's own prompt; the screens say why first. */
  voiceRequest(): Promise<VoiceAccess>;
  setPrivate(id: string, on: boolean): Promise<PhoneItem>;
  /** What this phone notes for itself, such as the reminder having been shown. */
  notes: NoteStore;
}

/** Below this much free room, Today, Keep and This phone say the phone is nearly full. */
export const NEARLY_FULL = 500_000_000;

function failed(e: unknown): HomeRequestError {
  return new HomeRequestError(typeof e === "string" ? (e as HomeError) : "failed");
}

async function local(call: HomeCall, onProgress?: Progress): Promise<HomeAnswer> {
  const stop = onProgress
    ? await listen<BackupProgress>("backup-progress", (e) => onProgress(e.payload))
    : null;
  try {
    const answer = await invoke<HomeAnswer>("home_local", { call });
    if (answer.type === "error") throw new HomeRequestError(answer.code);
    return answer;
  } catch (e) {
    throw e instanceof HomeRequestError ? e : failed(e);
  } finally {
    stop?.();
  }
}

function expect<K extends HomeAnswer["type"]>(
  answer: HomeAnswer,
  kind: K,
): Extract<HomeAnswer, { type: K }> {
  if (answer.type !== kind) throw new HomeRequestError("failed");
  return answer as Extract<HomeAnswer, { type: K }>;
}

export function createHomeApi(notes: NoteStore = idbNotes()): HomeApi {
  let current: PhoneMode | null = null;
  const listeners = new Set<(mode: PhoneMode | null) => void>();
  const set = (mode: PhoneMode | null) => {
    current = mode;
    for (const l of [...listeners]) l(mode);
  };
  void listen<PhoneMode | null>("mode_changed", (e) => set(e.payload)).catch(() => undefined);

  return {
    async mode() {
      current = await invoke<PhoneMode | null>("mode").catch(() => null);
      return current;
    },
    known: () => current,
    onMode(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async damaged() {
      return (await invoke<string | null>("memory_damaged").catch(() => null)) !== null;
    },
    async startAlone(name) {
      try {
        await invoke("start_alone", { name });
      } catch (e) {
        throw failed(e);
      }
      set("alone");
    },
    async find(query) {
      return expect(await local({ cmd: "find", query }), "found").items;
    },
    async stats() {
      return expect(await local({ cmd: "stats" }), "stats").stats;
    },
    async saveBackupFile(passphrase, onProgress) {
      const stop = await listen<BackupProgress>("backup-progress", (e) => onProgress(e.payload));
      try {
        const answer = await invoke<HomeAnswer | null>("save_backup_file", { passphrase });
        if (answer === null) return null;
        if (answer.type === "error") throw new HomeRequestError(answer.code);
        return expect(answer, "backupSaved").report;
      } catch (e) {
        throw e instanceof HomeRequestError ? e : failed(e);
      } finally {
        stop();
      }
    },
    async chooseBackupFile() {
      let chosen: ChosenBackup | null;
      try {
        chosen = await invoke<ChosenBackup | null>("choose_backup_file");
      } catch (e) {
        throw failed(e);
      }
      if (!chosen) return null;
      // No date: the file's own changes when it is copied, and the backup's is locked.
      return { path: chosen.file, name: chosen.name, bytes: chosen.size };
    },
    letGoBackupFile: () => invoke<void>("let_go_backup_file").catch(() => undefined),
    async checkBackup(file, passphrase, onProgress) {
      const { check, backup, here } = expect(
        await local({ cmd: "checkBackup", file, passphrase }, onProgress),
        "backupChecked",
      );
      return { check, backup, here };
    },
    async restoreBackup(file, passphrase, replace, onProgress) {
      expect(
        await local({ cmd: "restoreBackup", file, passphrase, replace }, onProgress),
        "restored",
      );
      set("alone");
    },
    async putAside() {
      return expect(await local({ cmd: "putAside" }), "putAside").memories;
    },
    async bringBack(id) {
      expect(await local({ cmd: "bringBack", id }), "restored");
    },
    async deletePutAside(id) {
      expect(await local({ cmd: "deletePutAside", id }), "putAsideDeleted");
    },
    async previewSourceDelete(source) {
      return expect(await local({ cmd: "previewSourceDelete", source }), "sourceDeletePreview")
        .preview;
    },
    async deleteSource(token, source, alsoStop) {
      expect(await local({ cmd: "deleteSource", token, source, alsoStop }), "sourceDeleted");
    },
    async setKeptHow(id, how) {
      expect(await local({ cmd: "setKeptHow", id, how }), "keptHowSet");
    },
    async itemKept(id) {
      return expect(await local({ cmd: "itemKept", id }), "itemKept").kept;
    },
    async previewForgetItem(id, withOriginal) {
      return expect(await local({ cmd: "previewForgetItem", id, withOriginal }), "forgetPreview")
        .preview;
    },
    async forget(token) {
      return expect(await local({ cmd: "forget", token }), "forgotten").report;
    },
    async deleteEverything(phrase) {
      expect(await local({ cmd: "deleteEverything", phrase }), "deleted");
      set(null);
    },
    freeSpace: () => invoke<number | null>("free_space").catch(() => null),
    voiceAccess: () => invoke<VoiceAccess>("voice_access").catch(() => "denied" as const),
    voiceRequest: () => invoke<VoiceAccess>("voice_request").catch(() => "denied" as const),
    async setPrivate(id, on) {
      return expect(await local({ cmd: "setPrivate", id, private: on }), "privateSet").item;
    },
    hearVoiceNotes: (language) =>
      invoke<boolean>("hear_voice_notes", { language }).catch(() => false),
    notes,
  };
}
