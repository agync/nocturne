// What the phone app's native side offers its screens: the calendar and health sources, what was
// last sent, and sending now. Types only; the app's Rust core and its plugins implement them.

import type { PhoneCalendar, PhoneFeature, ReadingKind, SyncReceipt } from "../../engine/bindings";

/**
 * A source on This phone. `offInSettings`: the permission was taken back in the phone's
 * settings. `stoppedOnComputer`: the computer is not taking it from this phone.
 * `needsNewerComputer`: the computer does not list it in its hello.
 */
export type SourceStatus =
  | "on"
  | "notBrought"
  | "offInSettings"
  | "stoppedOnComputer"
  | "needsNewerComputer";

/** "Your calendar": which calendars are brought, and notes on each appointment. */
export interface CalendarSettings {
  bring: boolean;
  /** Calendar id to on or off; a calendar not listed is on. */
  calendars: Record<string, boolean>;
  notes: boolean;
}

/** Health: the kinds brought, and whether older readings were asked for. */
export interface HealthSettings {
  bring: boolean;
  kinds: ReadingKind[];
  olderReadings: boolean;
}

/** "Last sent {time} · {count} calendar changes, {count} readings". */
export interface LastSent {
  at: string;
  calendarChanges: number;
  readings: number;
}

/** "Last sent {time} · {count} appointments" or "{count} readings", for one source. */
export interface SourceSent {
  at: string;
  count: number;
}

/** What one send did, per source; null for a source that sent nothing. */
export interface SyncReport {
  at: string;
  calendar: SyncReceipt | null;
  health: SyncReceipt | null;
}

/** What This phone shows under "What this phone brings". */
export interface ThisPhoneSources {
  calendar: SourceStatus;
  calendars: number;
  health: SourceStatus;
  kinds: number;
  lastSent: LastSent | null;
  calendarLastSent: SourceSent | null;
  healthLastSent: SourceSent | null;
  /** The first day Health was read from (`YYYY-MM-DD`), once it has been. */
  healthFrom: string | null;
  /** What the computer takes, from its hello. */
  computerTakes: PhoneFeature[];
  /** The last send had more appointments than the computer takes at once. */
  calendarTooMany: boolean;
}

/** The native side of calendar and health, as the screens call it. */
export interface NativeChannel {
  sources(): Promise<ThisPhoneSources>;
  /** The phone's calendars, as the phone names them. */
  calendars(): Promise<PhoneCalendar[]>;
  calendarSettings(): Promise<CalendarSettings>;
  setCalendarSettings(settings: CalendarSettings): Promise<ThisPhoneSources>;
  healthSettings(): Promise<HealthSettings>;
  setHealthSettings(settings: HealthSettings): Promise<ThisPhoneSources>;
  /** Sends what changed since the last send. */
  syncNow(): Promise<SyncReport>;
}
