import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ThisPhoneSources } from "./api";
import { CalendarSettingsScreen } from "./screens/CalendarConnect";
import { HealthSettingsScreen } from "./screens/HealthConnect";
import { nativeChannel } from "./sources";

afterEach(() => {
  cleanup();
  clearMocks();
});

const sources: ThisPhoneSources = {
  calendar: "on",
  calendars: 3,
  health: "on",
  kinds: 4,
  lastSent: null,
  calendarLastSent: null,
  healthLastSent: null,
  healthFrom: null,
  computerTakes: ["calendar", "health"],
  calendarTooMany: false,
};

const calendars = [
  { id: "1", name: "Rota", colour: "#3B7556", account: "maya.k@example.com" },
  { id: "2", name: "Holidays in Germany", colour: "#A8402A", account: "maya.k@example.com" },
  { id: "3", name: "Holidays in Germany", colour: "#6A4FA0", account: "Hospital" },
];

function phone() {
  mockIPC((cmd) => {
    if (cmd === "calendar_list") return calendars;
    if (cmd === "calendar_settings")
      return { bring: true, calendars: { "3": false }, notes: false };
    if (cmd === "health_settings")
      return { bring: true, kinds: ["sleep", "steps"], olderReadings: false };
    if (cmd === "calendar_save" || cmd === "health_save" || cmd === "sources") return sources;
    if (cmd === "health_older_available") return false;
  });
}

function lastInside(root: HTMLElement) {
  const focusable = root.querySelectorAll("button");
  return focusable[focusable.length - 1];
}

describe.each([
  ["calendar", "Show my calendar", "Bring my calendar"],
  ["health", "Show health readings", "Bring health readings"],
] as const)("the %s settings screen", (kind, aloneSwitch, companionSwitch) => {
  const open = (alone: boolean, onBack = () => undefined, onDeleteKept?: () => void) =>
    render(
      kind === "calendar" ? (
        <CalendarSettingsScreen
          channel={nativeChannel}
          onBack={onBack}
          alone={alone}
          onDeleteKept={onDeleteKept}
        />
      ) : (
        <HealthSettingsScreen
          channel={nativeChannel}
          onBack={onBack}
          alone={alone}
          onDeleteKept={onDeleteKept}
        />
      ),
    );

  it("on a phone alone: switch words, Kept so far above Done, and Done last", async () => {
    phone();
    const onBack = vi.fn();
    const onDelete = vi.fn();
    const { container } = open(true, onBack, onDelete);
    expect(await screen.findByRole("switch", { name: new RegExp(aloneSwitch) })).toBeTruthy();
    expect(screen.queryByRole("switch", { name: new RegExp(companionSwitch) })).toBeNull();
    const kept = screen.getByRole("region", { name: "Kept so far" });
    expect(within(kept).getByText("What Nocturne already kept stays until you delete it.")).toBeTruthy();
    fireEvent.click(within(kept).getByRole("button", { name: "Delete what was kept…" }));
    expect(onDelete).toHaveBeenCalledOnce();
    const done = screen.getByRole("button", { name: "Done" });
    expect(lastInside(container)).toBe(done);
    expect(kept.compareDocumentPosition(done) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Save|Cancel/ })).toBeNull();
    fireEvent.click(done);
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("with a computer: its own words, no Kept so far, Done last", async () => {
    phone();
    const onBack = vi.fn();
    const { container } = open(false, onBack);
    expect(await screen.findByRole("switch", { name: new RegExp(companionSwitch) })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Kept so far" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Delete what was kept/ })).toBeNull();
    expect(screen.getByText(/What already went across stays on your computer/)).toBeTruthy();
    const done = screen.getByRole("button", { name: "Done" });
    expect(lastInside(container)).toBe(done);
    fireEvent.click(done);
    expect(onBack).toHaveBeenCalledOnce();
  });
});

describe("the calendar list", () => {
  it("shows the account only for calendars that share a name", async () => {
    phone();
    render(<CalendarSettingsScreen channel={nativeChannel} onBack={() => undefined} alone />);
    const first = await screen.findByRole("switch", { name: /Holidays in Germany.*maya\.k/ });
    expect(first.textContent).toContain("maya.k@example.com");
    expect(screen.getByRole("switch", { name: /Holidays in Germany.*Hospital/ })).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Rota" }).textContent).not.toContain("maya.k");
  });

  it("words the note for each mode, and keeps saving as choices are made", async () => {
    const saved: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "calendar_list") return calendars;
      if (cmd === "calendar_settings") return { bring: true, calendars: {}, notes: false };
      if (cmd === "calendar_save") {
        saved.push((args as { settings: unknown }).settings);
        return sources;
      }
      if (cmd === "sources") return sources;
    });
    const alone = render(
      <CalendarSettingsScreen channel={nativeChannel} onBack={() => undefined} alone />,
    );
    expect(
      await screen.findByText("Every calendar starts on. Turn off any you don’t want in Nocturne."),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("switch", { name: "Rota" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    alone.unmount();
    render(<CalendarSettingsScreen channel={nativeChannel} onBack={() => undefined} />);
    expect(
      await screen.findByText(
        "Every calendar starts on. Turn off any you don’t want on your page.",
      ),
    ).toBeTruthy();
  });
});
