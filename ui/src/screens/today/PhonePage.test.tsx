import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const NOW = "2026-11-13T07:02:00Z";
const open = async () => {
  const engine = withPhones(createMockEngine(), NOW);
  renderApp({ hash: "#/today", engine });
  await screen.findByRole("heading", { level: 1 });
  return engine;
};

describe("the morning page with a phone's calendar and last night", () => {
  it("shows what is on today with its time, place and receipt", async () => {
    await open();
    const section = (await screen.findByRole("heading", { name: "In your calendar today · 3" }))
      .parentElement as HTMLElement;
    const rows = within(section).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("9:00 am");
    expect(rows[0]).toHaveTextContent("Dentist, check-up");
    expect(rows[0]).toHaveTextContent("Bridge Street Surgery");
    expect(
      within(rows[0] as HTMLElement).getByRole("button", {
        name: "Calendar · Family · Sam’s phone. Source.",
      }),
    ).toBeInTheDocument();
  });

  it("opens the entry's own page from its receipt", async () => {
    await open();
    const [chip] = await screen.findAllByRole("button", {
      name: "Calendar · Family · Sam’s phone. Source.",
    });
    await userEvent.click(chip as HTMLElement);
    expect(window.location.hash).toBe("#/items/cal-dentist");
  });

  it("puts last night under the title with its bar and a receipt to the night", async () => {
    await open();
    const night = (await screen.findByRole("heading", { name: "Last night" })).closest(
      "section",
    ) as HTMLElement;
    expect(night).toHaveTextContent("6 h 40 min asleep, 11:50 pm to 7:05 am");
    expect(
      within(night).getByRole("img", {
        name: "6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once.",
      }),
    ).toBeInTheDocument();
    await userEvent.click(
      within(night).getByRole("button", { name: "Sleep · Health · Sam’s phone. Source." }),
    );
    expect(window.location.hash).toBe("#/items/read-sleep-0");
  });

  it("puts last night before the deck and the calendar before what changed", async () => {
    await open();
    const order = (await screen.findAllByRole("heading")).map((h) => h.textContent);
    expect(order.indexOf("Last night")).toBeGreaterThan(-1);
    expect(order.indexOf("Last night")).toBeLessThan(order.indexOf("In your calendar today · 3"));
    expect(order.indexOf("In your calendar today · 3")).toBeLessThan(
      order.findIndex((h) => h?.startsWith("What changed")),
    );
  });

  it("shows neither when the person turned them off, and neither when nothing is brought", async () => {
    const engine = withPhones(createMockEngine(), NOW);
    await engine.setPhonePageOptions({ calendarToday: false, sleepLastNight: false });
    renderApp({ hash: "#/today", engine });
    await screen.findByRole("heading", { level: 1 });
    expect(screen.queryByText(/In your calendar today/)).toBeNull();
    expect(screen.queryByText("Last night")).toBeNull();
  });

  it("is the ordinary page without any phone", async () => {
    renderApp({ hash: "#/today", engine: createMockEngine() });
    await screen.findByRole("heading", { level: 1 });
    expect(screen.queryByText(/In your calendar today/)).toBeNull();
  });
});

describe("read aloud", () => {
  it("never lists last night or the calendar among what is read", async () => {
    const engine = withPhones(createMockEngine(), NOW);
    renderApp({ hash: "#/listen/2026-11-13", engine });
    const list = await screen.findByRole("list", { name: "Sections" });
    expect(list).not.toHaveTextContent(/last night|asleep|calendar/i);
    expect(screen.queryByText(/asleep/)).toBeNull();
  });
});
