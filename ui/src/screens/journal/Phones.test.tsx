import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";
import { FromHealth } from "./FromHealth";

const NOW = new Date("2026-11-13T07:02:00Z");

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const engine = () =>
  withPhones(createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 0 } }), NOW);

async function open() {
  const e = engine();
  renderApp({ hash: "#/journal", engine: e });
  await screen.findByRole("heading", { level: 2, name: "Through the day" });
  return e;
}

const timeline = () =>
  screen.getByRole("heading", { level: 2, name: "Through the day" }).parentElement as HTMLElement;

describe("the journal day with a phone's calendar and health", () => {
  it("puts the night, the calendar and a workout on the timeline", async () => {
    await open();
    const rows = within(timeline());
    expect(rows.getByText("Woke · 6 h 40 min asleep")).toBeInTheDocument();
    expect(rows.getByText("Dentist, check-up · Bridge Street Surgery")).toBeInTheDocument();
    expect(rows.getByText("Walk · 42 min · 3.1 km")).toBeInTheDocument();
    expect(rows.getAllByText("from your calendar")).toHaveLength(2);
    expect(rows.getByRole("button", { name: /Sleep · Health · Sam’s phone/ })).toBeInTheDocument();
  });

  it("says how long the first check-in came after sleeping", async () => {
    await open();
    expect(
      within(timeline()).getByRole("link", { name: "after 6 h 40 min asleep", hidden: true }),
    ).toHaveAttribute("href", "#/journal/health/sleep?day=2026-11-13");
  });

  it("shows From Health beside the day, each part with its sentence and a way to see more", async () => {
    await open();
    const block = screen.getByRole("heading", { name: "From Health" }).parentElement as HTMLElement;
    const side = within(block);
    expect(
      side.getByRole("img", {
        name: "6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once.",
      }),
    ).toBeInTheDocument();
    expect(
      side.getByRole("img", {
        name: "Resting heart rate, 58 beats a minute today. Your usual is 55 to 61.",
      }),
    ).toBeInTheDocument();
    expect(
      side.getByRole("img", { name: "8,412 steps today. 6,900 on an average day this fortnight." }),
    ).toBeInTheDocument();
    expect(
      side.getByRole("img", {
        name: "Heart rate today, 52 to 148 beats a minute, mostly around 74.",
      }),
    ).toBeInTheDocument();
    expect(side.getByText("128 over 82 · 7:40 am")).toBeInTheDocument();
    const more = side.getAllByRole("link", { name: /See more/ });
    expect(more.map((a) => a.getAttribute("href"))).toContain("#/journal/health/sleep");
  });

  it("shows nothing from Health when no phone brings it", async () => {
    const e = engine();
    await e.setPhoneSource("pixel", "health", false);
    renderApp({ hash: "#/journal", engine: e });
    await screen.findByRole("heading", { level: 2, name: "Through the day" });
    expect(screen.queryByRole("heading", { name: "From Health" })).toBeNull();
    expect(within(timeline()).queryByText(/Woke ·/)).toBeNull();
    expect(within(timeline()).getByText("Dentist, check-up · Bridge Street Surgery")).toBeVisible();
  });
});

describe("Delete this day with a phone's things", () => {
  it("lists the calendar and the readings in their own groups, unticked", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "About this day" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete this day…" }));
    const dialog = await screen.findByRole("dialog", { name: /^Delete Friday 13 November\?/ });
    const calendar = await within(dialog).findByRole("region", { name: "From your calendar" });
    const health = within(dialog).getByRole("region", { name: "From Health" });
    for (const box of within(calendar).getAllByRole("checkbox")) expect(box).not.toBeChecked();
    for (const box of within(health).getAllByRole("checkbox")) expect(box).not.toBeChecked();
    expect(within(calendar).getByText("Dentist, check-up")).toBeInTheDocument();
    expect(within(health).getByText(/Walk · 42 min/)).toBeInTheDocument();
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: /^Delete \d+ things?$/ })).toBeEnabled(),
    );
  });
});

describe("From Health on a day that is not today", () => {
  it("never says today", async () => {
    const e = engine();
    const health = (await e.getJournalDay()).health;
    if (!health) throw new Error("no health");
    render(<FromHealth health={health} day="2026-11-12" isToday={false} />);
    const text = document.body.textContent ?? "";
    expect(text).not.toMatch(/today/i);
    expect(
      screen.getByRole("img", {
        name: /^Resting heart rate, 58 beats a minute on Friday 13 November\./,
      }),
    ).toBeInTheDocument();
  });
});

describe("a night and a workout on the timeline", () => {
  it("are read on their Health page, not on a page to keep, read again or forget", async () => {
    await open();
    const woke = within(timeline()).getByText("Woke · 6 h 40 min asleep").closest("a");
    expect(woke).toHaveAttribute("href", "#/journal/health/sleep?day=2026-11-13");
    const walk = within(timeline()).getByText("Walk · 42 min · 3.1 km").closest("a");
    expect(walk).toHaveAttribute("href", "#/journal/health/workouts?day=2026-11-13");
    const entry = within(timeline())
      .getByText(/^Dentist, check-up/)
      .closest("a");
    expect(entry).toHaveAttribute("href", "#/items/cal-dentist?from=journal");
  });
});
