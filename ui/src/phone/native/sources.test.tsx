import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ThisPhoneSources } from "./api";
import { routeOf } from "./NativeRoot";
import { CalendarConnect } from "./screens/CalendarConnect";
import { fromDay, HealthConnect, kindList } from "./screens/HealthConnect";
import { Rationale } from "./screens/Rationale";
import { askCalendar, askHealth, nativeChannel, takeLaunchRoute } from "./sources";

afterEach(() => {
  cleanup();
  clearMocks();
});

/** Sent at 7:42 this morning, so the line reads as today's, whatever day the test runs. */
const sentToday = new Date(new Date().setHours(7, 42, 0, 0)).toISOString();

const sources: ThisPhoneSources = {
  calendar: "on",
  calendars: 1,
  health: "notBrought",
  kinds: 0,
  lastSent: { at: sentToday, calendarChanges: 23, readings: 0 },
  calendarLastSent: { at: sentToday, count: 23 },
  healthLastSent: null,
  healthFrom: null,
  computerTakes: ["calendar", "health"],
  calendarTooMany: false,
};

const calendars = [
  { id: "1", name: "Family", colour: "#3F51B5", kind: "ordinary" },
  { id: "2", name: "Holidays in England", colour: null, kind: "holidays" },
];

describe("the phone's sources through the Rust side", () => {
  it("names each command and its arguments", async () => {
    const seen: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      seen.push([cmd, args]);
      if (cmd === "sources") return sources;
      if (cmd === "calendar_save") return sources;
      if (cmd === "sync_now") return null;
      if (cmd === "calendar_request") return "granted";
      if (cmd === "health_request") return { kinds: ["sleep"], history: false };
      if (cmd === "launch_route") return "rationale";
    });
    expect(await nativeChannel.sources()).toEqual(sources);
    const settings = { bring: true, calendars: { "2": true }, notes: false };
    await nativeChannel.setCalendarSettings(settings);
    expect(seen).toContainEqual(["calendar_save", { settings }]);
    expect(await askCalendar()).toBe("granted");
    expect(await askHealth(["sleep", "steps"], true)).toEqual({ kinds: ["sleep"], history: false });
    expect(seen).toContainEqual(["health_request", { kinds: ["sleep", "steps"], history: true }]);
    expect(await takeLaunchRoute()).toBe("rationale");
  });

  it("answers an empty report when nothing could be sent", async () => {
    mockIPC((cmd) => (cmd === "sync_now" ? null : undefined));
    const report = await nativeChannel.syncNow();
    expect(report.calendar).toBeNull();
    expect(report.health).toBeNull();
  });

  it("reads a failed prompt as refused for good", async () => {
    mockIPC((cmd) => {
      if (cmd === "calendar_request") throw "failed";
      if (cmd === "health_request") throw "failed";
    });
    expect(await askCalendar()).toBe("blocked");
    expect(await askHealth(["sleep"], false)).toEqual({ kinds: [], history: false });
  });

  it("finds the pages opened by their address", () => {
    expect(routeOf("#/rationale")).toBe("rationale");
    expect(routeOf("#calendar")).toBe("calendar");
    expect(routeOf("#/health")).toBe("health");
    expect(routeOf("")).toBeNull();
    expect(routeOf("#/elsewhere")).toBeNull();
  });
});

describe("bringing the calendar", () => {
  it("asks after Nocturne's own screen, then shows which calendars", async () => {
    const saved: unknown[] = [];
    let access = "ask";
    mockIPC((cmd, args) => {
      if (cmd === "calendar_status") return access;
      if (cmd === "calendar_request") {
        access = "granted";
        return access;
      }
      if (cmd === "calendar_settings")
        return { bring: saved.length > 0, calendars: { "1": true, "2": false }, notes: false };
      if (cmd === "calendar_save") {
        saved.push((args as { settings: unknown }).settings);
        return sources;
      }
      if (cmd === "calendar_list") return calendars;
      if (cmd === "sources") return sources;
    });
    render(<CalendarConnect channel={nativeChannel} onDone={() => undefined} />);
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe(
      "Bring your calendar to your computer?",
    );
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const holidays = await screen.findByRole("switch", { name: /Holidays in England/ });
    expect(holidays.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("switch", { name: /Family/ }).getAttribute("aria-checked")).toBe(
      "true",
    );
    expect(
      screen
        .getByRole("switch", { name: /Notes on each appointment/ })
        .getAttribute("aria-checked"),
    ).toBe("false");
    expect(saved).toEqual([{ bring: true, calendars: { "1": true, "2": false }, notes: false }]);
    expect(screen.getByText("Last sent 7:42 am · 23 appointments", { exact: false })).toBeTruthy();
  });

  it("stays on the phone when refused, and offers settings once the phone won't ask", async () => {
    mockIPC((cmd) => {
      if (cmd === "calendar_status") return "blocked";
      if (cmd === "calendar_settings") return { bring: false, calendars: {}, notes: false };
    });
    render(<CalendarConnect channel={nativeChannel} onDone={() => undefined} />);
    expect(await screen.findByRole("button", { name: "Open phone settings" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Continue" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe(
      "Your calendar stays on this phone.",
    );
    expect(screen.getByText(/You can bring it later in/).textContent).toContain("This phone");
  });
});

describe("bringing Health", () => {
  it("chooses kinds with weight off, then says what is brought", async () => {
    let asked: unknown = null;
    let saved: unknown = null;
    mockIPC((cmd, args) => {
      if (cmd === "health_availability") return "available";
      if (cmd === "health_settings")
        return {
          bring: false,
          kinds: ["sleep", "heartRate", "restingHeartRate", "bloodPressure", "steps", "workout"],
          olderReadings: false,
        };
      if (cmd === "sources") return sources;
      if (cmd === "health_request") {
        asked = args;
        return { kinds: ["sleep", "steps"], history: false };
      }
      if (cmd === "health_save") {
        saved = (args as { settings: unknown }).settings;
        return sources;
      }
    });
    render(<HealthConnect channel={nativeChannel} onDone={() => undefined} />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(
      (await screen.findByRole("switch", { name: /Weight/ })).getAttribute("aria-checked"),
    ).toBe("false");
    fireEvent.click(screen.getByRole("switch", { name: /Blood pressure/ }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
        "Bringing sleep and steps.",
      ),
    );
    expect(asked).toEqual({
      kinds: ["sleep", "heartRate", "restingHeartRate", "steps", "workout"],
      history: false,
    });
    expect(saved).toMatchObject({ bring: true });
  });

  it("asks for Health Connect where it is missing", async () => {
    mockIPC((cmd) => {
      if (cmd === "health_availability") return "missing";
      if (cmd === "health_settings") return { bring: false, kinds: [], olderReadings: false };
      if (cmd === "sources") return sources;
    });
    render(<HealthConnect channel={nativeChannel} onDone={() => undefined} />);
    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe(
      "Health Connect is needed",
    );
    expect(screen.getByRole("button", { name: "Get Health Connect" })).toBeTruthy();
  });

  it("says the first day read, with the year when it is a year back", () => {
    const now = new Date("2026-11-21T10:00:00");
    expect(fromDay("2026-10-23", false, now)).toBe("23 October");
    expect(fromDay("2025-11-22", true, now)).toBe("22 November 2025");
    expect(fromDay(null, false, now)).toBe("23 October");
  });

  it("lists kinds the way a sentence says them", () => {
    expect(kindList(["steps", "sleep", "heartRate"])).toBe("sleep, heart rate and steps");
  });
});

describe("the rationale page", () => {
  it("says what is read, why, where it goes, and how to stop", () => {
    render(<Rationale onBack={() => undefined} />);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "How Nocturne uses your health readings",
    );
    expect(screen.getByText(/It never writes to Health Connect/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Read the full privacy policy" })).toBeTruthy();
  });
});
