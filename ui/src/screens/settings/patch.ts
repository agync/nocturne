import type { SettingsPatch } from "../../engine/bindings";

/** A settings patch with only the named fields set; null means "leave as it is". */
export function patch(fields: Partial<SettingsPatch>): SettingsPatch {
  return {
    pageTime: null,
    delivery: null,
    autoPrint: null,
    voice: null,
    voiceSpeed: null,
    shortcut: null,
    inboxFolder: null,
    startAtLogin: null,
    lettingGo: null,
    theme: null,
    language: null,
    checkUpdates: null,
    reminders: null,
    lock: null,
    agents: null,
    ...fields,
  };
}
