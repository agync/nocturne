import type { SettingsView } from "../../bindings";

export function settingsSeed(): SettingsView {
  return {
    pageTime: "07:00",
    delivery: "notify",
    autoPrint: false,
    voice: "bf_emma",
    voiceSpeed: 1,
    shortcut: "Ctrl+Shift+Space",
    shortcutAvailable: true,
    desktopShortcut: null,
    inboxFolder: null,
    startAtLogin: true,
    lettingGo: "normal",
    theme: "system",
    language: "en-GB",
    checkUpdates: false,
    reminders: false,
    lock: false,
    agents: false,
  };
}
