import type { SettingsView } from "../../bindings";

export function settingsSeed(): SettingsView {
  return {
    pageTime: "07:00",
    delivery: "notify",
    autoPrint: false,
    voice: "isla",
    voiceSpeed: 1,
    shortcut: "Ctrl+Shift+Space",
    shortcutAvailable: true,
    inboxFolder: null,
    startAtLogin: true,
    lettingGo: "gentle",
    theme: "system",
    language: "en-GB",
    checkUpdates: false,
    reminders: false,
    lock: false,
    agents: false,
  };
}
