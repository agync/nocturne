// The phone's own 12 or 24-hour setting, which every time on its screens follows. Only the app's
// native side can read it; where it cannot, times read in twelve hours.

import { invoke } from "@tauri-apps/api/core";
import { setHourClock } from "../../copy/format";

/** Fired on the window when the phone's setting changed while the app was away. */
export const CLOCK_CHANGED = "ph-clock";

/** Whether the phone is set to 24 hours; null where the app cannot tell. */
export function phoneIs24Hour(): Promise<boolean | null> {
  return invoke<boolean | null>("plugin:nocturne-system|hour_clock").catch(() => null);
}

/**
 * Reads the setting now and each time the app comes back to the front, where it may have
 * changed; the screens draw again at once when it has.
 */
export async function startClock(): Promise<void> {
  let current = await phoneIs24Hour();
  setHourClock(current);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") return;
    void phoneIs24Hour().then((now) => {
      if (now === current) return;
      current = now;
      setHourClock(now);
      window.dispatchEvent(new Event(CLOCK_CHANGED));
    });
  });
}
