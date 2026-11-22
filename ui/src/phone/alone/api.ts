// What a phone alone asks of its own memory beyond the shared screen calls: where the memory
// lives, starting, finding by words, backups and deleting everything. Each goes to the app's Rust
// side; backups report progress as `backup-progress` events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  BackupProgress,
  BackupReport,
  FoundItem,
  HomeAnswer,
  HomeCall,
  HomeError,
  HomeStats,
  PhoneItem,
  PhoneMode,
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

/** A backup file the person chose. */
export interface PickedFile {
  path: string;
  name: string;
  bytes: number;
  /** When the backup was made, when the sheet can tell before the passphrase. */
  madeAt?: string;
}

/**
 * The phone's own sheets for choosing where a backup goes and which one comes back. Each
 * resolves to null when the person chose nothing.
 */
export interface HomeFiles {
  saveTo(suggested: string): Promise<string | null>;
  open(): Promise<PickedFile | null>;
}

/** No sheet to choose with: nothing is ever chosen. */
export const noFiles: HomeFiles = {
  saveTo: () => Promise.resolve(null),
  open: () => Promise.resolve(null),
};

export type Progress = (p: BackupProgress) => void;

/** Whether the phone's speech reader may run, as the speech plugin says. */
export type VoiceAccess = "granted" | "denied" | "prompt" | "prompt-with-rationale";

export interface HomeApi {
  /** Where the memory lives; null before the person has chosen. */
  mode(): Promise<PhoneMode | null>;
  /** The last mode heard, without asking. */
  known(): PhoneMode | null;
  /** Called with each new mode, after a start or a restore. */
  onMode(listener: (mode: PhoneMode | null) => void): () => void;
  /** Set when the memory could not be opened and was put aside. */
  damaged(): Promise<boolean>;
  startAlone(name: string): Promise<void>;
  find(query: string): Promise<FoundItem[]>;
  stats(): Promise<HomeStats>;
  saveBackup(dest: string, passphrase: string, onProgress: Progress): Promise<BackupReport>;
  checkBackup(file: string, passphrase: string, onProgress: Progress): Promise<RestoreCheck>;
  restoreBackup(
    file: string,
    passphrase: string,
    replace: boolean,
    onProgress: Progress,
  ): Promise<void>;
  deleteEverything(phrase: string): Promise<void>;
  /** The phone's language, so waiting voice notes can be read into words where it can. */
  hearVoiceNotes(language: string): Promise<boolean>;
  voiceAccess(): Promise<VoiceAccess>;
  /** The phone's own prompt; the screens say why first. */
  voiceRequest(): Promise<VoiceAccess>;
  setPrivate(id: string, on: boolean): Promise<PhoneItem>;
  files: HomeFiles;
  /** What this phone notes for itself, such as the reminder having been shown. */
  notes: NoteStore;
}

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

export function createHomeApi(files: HomeFiles = noFiles, notes: NoteStore = idbNotes()): HomeApi {
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
    async saveBackup(dest, passphrase, onProgress) {
      return expect(await local({ cmd: "saveBackup", dest, passphrase }, onProgress), "backupSaved")
        .report;
    },
    async checkBackup(file, passphrase, onProgress) {
      return expect(
        await local({ cmd: "checkBackup", file, passphrase }, onProgress),
        "backupChecked",
      ).check;
    },
    async restoreBackup(file, passphrase, replace, onProgress) {
      expect(
        await local({ cmd: "restoreBackup", file, passphrase, replace }, onProgress),
        "restored",
      );
      set("alone");
    },
    async deleteEverything(phrase) {
      expect(await local({ cmd: "deleteEverything", phrase }), "deleted");
    },
    voiceAccess: () => invoke<VoiceAccess>("voice_access").catch(() => "denied" as const),
    voiceRequest: () => invoke<VoiceAccess>("voice_request").catch(() => "denied" as const),
    async setPrivate(id, on) {
      return expect(await local({ cmd: "setPrivate", id, private: on }), "privateSet").item;
    },
    hearVoiceNotes: (language) =>
      invoke<boolean>("hear_voice_notes", { language }).catch(() => false),
    files,
    notes,
  };
}
