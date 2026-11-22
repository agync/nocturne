// The phone app's scripted build for the browser tests: the scripted phone engine, and the app's
// native side answered in the page. `?s=` picks the connection as on the phone page, `?n=` what
// the phone's own side says, `?v=` the screen it opens on, and `?m=` what a phone alone holds.
// Nothing here is in the shipped app.

import { mockIPC } from "@tauri-apps/api/mocks";
import type { PhoneCalendar } from "../../engine/bindings";
import { createHomeApi } from "../alone/api";
import { createLocalEngine } from "../alone/engine";
import {
  type HomeScenario,
  homeState,
  scriptedNotes,
  seedReminder,
} from "../alone/testing/homeMock";
import type { CalendarSettings, HealthSettings, ThisPhoneSources } from "../native/api";
import type { Shared } from "../native/launch";
import type { NativeServices } from "../native/services";
import { stillGranted } from "../native/sources";
import type { NativeKeyStore } from "../native/store";
import { TODAY } from "./journalFixtures";
import { mockServices } from "./mockServices";
import { withCalendar, withPhones } from "./phoneFixtures";

const at = (time: string, day = TODAY) => `${day}T${time}:00Z`;

const CALENDARS: PhoneCalendar[] = [
  { id: "1", name: "Family", colour: "#3B7556" },
  { id: "2", name: "Work", colour: "#2C4BCF" },
  { id: "3", name: "School term dates", colour: "#8A6A12" },
  { id: "4", name: "Holidays in England", colour: "#A8402A" },
];

export type NativeScenario =
  | "full"
  | "fresh"
  | "calendarRefused"
  | "healthMissing"
  | "revoked"
  | "stopped"
  | "share"
  | "shareFile"
  | "older";

function sourcesFor(n: NativeScenario): ThisPhoneSources {
  const fresh = n === "fresh" || n === "calendarRefused" || n === "healthMissing";
  return {
    calendar: fresh
      ? "notBrought"
      : n === "revoked"
        ? "offInSettings"
        : n === "stopped"
          ? "stoppedOnComputer"
          : "on",
    calendars: 3,
    health: fresh ? "notBrought" : "on",
    kinds: 6,
    lastSent: fresh ? null : { at: at("07:42"), calendarChanges: 4, readings: 18 },
    calendarLastSent: fresh ? null : { at: at("07:42"), count: 23 },
    healthLastSent: fresh ? null : { at: at("07:42"), count: 18 },
    healthFrom: fresh ? null : "2026-10-20",
    computerTakes: n === "older" ? [] : ["calendar", "health"],
    calendarTooMany: false,
  };
}

/** A one-pixel photo, as another app might share it. */
const PNG = Uint8Array.from(
  atob(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  ),
  (c) => c.charCodeAt(0),
);

function sharedFor(n: NativeScenario): Shared | null {
  if (n === "share")
    return {
      text: "https://example.org/school-trip-letter",
      subject: "Year 4 trip letter",
      files: [],
    };
  if (n === "shareFile")
    return {
      text: null,
      subject: null,
      files: [
        { name: "0-letter.png", shown: "School letter.png", mime: "image/png", size: 2400000 },
      ],
    };
  return null;
}

/** The app's native side, answered in the page. */
export function installNativeIpc(
  n: NativeScenario,
  home?: (cmd: string, args: Record<string, unknown>) => unknown,
): void {
  const sources = sourcesFor(n);
  let calendar: CalendarSettings = {
    bring: sources.calendar !== "notBrought",
    calendars: { "1": true, "2": true, "3": true },
    notes: false,
  };
  let health: HealthSettings = {
    bring: sources.health === "on",
    kinds: ["sleep", "heartRate", "restingHeartRate", "bloodPressure", "steps", "workout"],
    olderReadings: false,
  };
  let shared = sharedFor(n);
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "sources":
          return sources;
        case "calendar_list":
          return CALENDARS;
        case "calendar_settings":
          return calendar;
        case "calendar_save":
          calendar = a.settings as CalendarSettings;
          sources.calendar = calendar.bring ? "on" : "notBrought";
          return sources;
        case "calendar_status":
          return n === "revoked"
            ? "blocked"
            : sources.calendar === "notBrought"
              ? "ask"
              : "granted";
        case "calendar_request":
          return n === "calendarRefused" ? "refused" : "granted";
        case "health_availability":
          return n === "healthMissing" ? "missing" : "available";
        case "permissions_granted": {
          const given = new Set((new URLSearchParams(location.search).get("g") ?? "").split(","));
          return {
            calendar: given.has("calendar"),
            health: given.has("health"),
            microphone: given.has("microphone"),
          };
        }
        case "health_older_available":
          return false;
        case "health_settings":
          return health;
        case "health_save":
          health = a.settings as HealthSettings;
          sources.health = health.bring ? "on" : "notBrought";
          sources.kinds = health.kinds.length;
          return sources;
        case "health_request":
          return { kinds: a.kinds, history: false };
        case "sync_now":
        case "last_sent":
        case "launch_route":
          return null;
        case "plugin:nocturne-system|insets":
          return { top: 0, right: 0, bottom: 0, left: 0, keyboard: 0 };
        case "plugin:nocturne-system|hour_clock": {
          const clock = new URLSearchParams(location.search).get("h");
          return clock === null ? null : clock === "24";
        }
        case "plugin:nocturne-system|device_name":
          return "Pixel 8";
        case "plugin:nocturne-system|take_launch":
          return null;
        case "plugin:nocturne-system|take_shared": {
          const once = shared;
          shared = null;
          return once;
        }
        case "plugin:nocturne-system|shared_file":
          return Array.from(PNG);
        default:
          return home?.(cmd, a) ?? null;
      }
    },
    { shouldMockEvents: true },
  );
}

/** The scripted services, as the app's: the same engine, with a store that says when it changed. */
export async function nativeMockServices(): Promise<NativeServices> {
  const params = new URLSearchParams(location.search);
  const alone = params.get("m") as HomeScenario | null;
  installNativeIpc(
    (params.get("n") ?? "full") as NativeScenario,
    alone ? homeState(alone).answer : undefined,
  );
  const home = createHomeApi(scriptedNotes);
  const view = params.get("v");
  if (view)
    history.replaceState(
      {
        view:
          view === "offer" || view === "thisPhone" || view === "keep"
            ? { name: view }
            : { name: "tab", tab: view },
      },
      "",
    );
  const base = await mockServices();
  if (alone === "reminder") seedReminder(base.now());
  const listeners = new Set<() => void>();
  const store: NativeKeyStore = {
    device: () => base.store.device(),
    save: (d) => base.store.save(d),
    async clear() {
      await base.store.clear();
      for (const l of listeners) l();
    },
    changed() {
      for (const l of listeners) l();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return {
    ...base,
    store,
    home,
    engineFor: (address) => {
      if (home.known() === "alone")
        return createLocalEngine({ deviceName: () => "Maya’s phone", version: base.version });
      const engine = base.engineFor(address);
      return {
        ...engine,
        getPage: async (date) => withCalendar(await engine.getPage(date)),
        getJournalDay: async (date) => {
          const day = await engine.getJournalDay(date);
          return day.today ? withPhones(day) : day;
        },
      };
    },
    platform: { ios: false, standalone: true, native: true },
    granted: stillGranted,
    deviceName: async () => (alone ? "Maya’s phone" : "Pixel 8"),
  };
}
