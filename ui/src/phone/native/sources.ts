// The calendar and Health on This phone, through the app's Rust side: what each source stands at,
// the person's choices, the phone's own prompts, and sending now. Only Rust reads the sources and
// reaches the computer.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PhoneCalendar, ReadingKind } from "../../engine/bindings";
import type {
  CalendarSettings,
  HealthSettings,
  NativeChannel,
  SyncReport,
  ThisPhoneSources,
} from "./api";

/**
 * Whether the app may read the calendar. `ask`: never asked. `refused`: refused once, and the
 * phone can still ask. `blocked`: the phone will no longer show its prompt, or the permission was
 * taken back in its settings.
 */
export type CalendarAccess = "granted" | "ask" | "refused" | "blocked";

/** Whether Health Connect is on this phone. */
export type HealthAvailability = "available" | "needsUpdate" | "missing";

/** The kinds Health Connect allows, and whether it allows older readings. */
export interface HealthGranted {
  kinds: ReadingKind[];
  history: boolean;
}

/** The kinds in the order the design lists them; weight is offered and starts off. */
export const KINDS: ReadingKind[] = [
  "sleep",
  "heartRate",
  "restingHeartRate",
  "bloodPressure",
  "steps",
  "workout",
  "weight",
];

export const DEFAULT_KINDS: ReadingKind[] = KINDS.filter((k) => k !== "weight");

export const nativeChannel: NativeChannel = {
  sources: () => invoke<ThisPhoneSources>("sources"),
  calendars: () => invoke<PhoneCalendar[]>("calendar_list"),
  calendarSettings: () => invoke<CalendarSettings>("calendar_settings"),
  setCalendarSettings: (settings) => invoke<ThisPhoneSources>("calendar_save", { settings }),
  healthSettings: () => invoke<HealthSettings>("health_settings"),
  setHealthSettings: (settings) => invoke<ThisPhoneSources>("health_save", { settings }),
  async syncNow(): Promise<SyncReport> {
    const report = await invoke<SyncReport | null>("sync_now", { asked: true });
    return report ?? { at: new Date().toISOString(), calendar: null, health: null };
  },
};

export function calendarAccess(): Promise<CalendarAccess> {
  return invoke<CalendarAccess>("calendar_status");
}

/** The phone's own prompt; what it says. */
export function askCalendar(): Promise<CalendarAccess> {
  return invoke<CalendarAccess>("calendar_request").catch(() => "blocked" as const);
}

export function healthAvailability(): Promise<HealthAvailability> {
  return invoke<HealthAvailability>("health_availability");
}

/** Whether this phone's Health Connect has readings older than 30 days to give. */
export function healthOlderAvailable(): Promise<boolean> {
  return invoke<boolean>("health_older_available").catch(() => false);
}

/** Health Connect's own screen, listing only the kinds chosen. */
export function askHealth(kinds: ReadingKind[], history: boolean): Promise<HealthGranted> {
  return invoke<HealthGranted>("health_request", { kinds, history }).catch(() => ({
    kinds: [],
    history: false,
  }));
}

/** Health Connect's settings, or Health Connect in the store where it is missing. */
export function openHealth(target: "health" | "healthStore"): Promise<void> {
  return invoke<void>("open_settings", { target }).catch(() => undefined);
}

/** This app's page in the phone's settings, where the calendar is allowed again. */
export function openAppSettings(): Promise<void> {
  return invoke<void>("plugin:nocturne-system|open_app_settings").catch(() => undefined);
}

/** Each send's report, as it happens. */
export function onSyncReport(handler: (report: SyncReport) => void): Promise<UnlistenFn> {
  return listen<SyncReport>("sync-report", (e) => handler(e.payload));
}

/** The page the app was opened on from outside (`rationale`, from Health Connect), once. */
export function takeLaunchRoute(): Promise<string | null> {
  return invoke<string | null>("launch_route").catch(() => null);
}

/**
 * Each time the app comes back to the front: a send, which the Rust side skips if one just ran,
 * and a look for a page it was opened on.
 */
export function watchFront(onRoute: (route: string) => void): () => void {
  const check = () => {
    void takeLaunchRoute().then((route) => route && onRoute(route));
  };
  const seen = () => {
    if (document.visibilityState !== "visible") return;
    void invoke("sync_now", { asked: false }).catch(() => undefined);
    check();
  };
  check();
  document.addEventListener("visibilitychange", seen);
  return () => document.removeEventListener("visibilitychange", seen);
}
