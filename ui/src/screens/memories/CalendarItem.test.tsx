import { screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const NOW = "2026-11-13T07:02:00Z";
const open = async (id: string) => {
  renderApp({ hash: `#/items/${id}`, engine: withPhones(createMockEngine(), NOW) });
  return screen.findByRole("heading", { level: 1 });
};

describe("a calendar entry's own page", () => {
  it("says when, where, who and which calendar, and what it was before", async () => {
    const title = await open("cal-leo-swim");
    expect(title).toHaveTextContent("Leo, swimming");
    expect(screen.getByText("Calendar · Family · Sam’s phone")).toBeInTheDocument();
    const facts = screen.getAllByText(/Friday 13 November, 4:30 pm to 5:15 pm/);
    expect(facts.length).toBeGreaterThan(0);
    expect(screen.getByText("Every Friday")).toBeInTheDocument();
    expect(screen.getByText("Riverside pool")).toBeInTheDocument();
    expect(screen.getByText("Sam, Leo")).toBeInTheDocument();
    expect(screen.getByText("Who’s invited")).toBeInTheDocument();
    expect(
      screen.getByText("From Sam’s phone’s calendar · last updated 7:42 am"),
    ).toBeInTheDocument();
    expect(screen.getByText("Earlier versions")).toBeInTheDocument();
    expect(screen.getByText("Was Wednesday 11 November, 4:30 pm")).toBeInTheDocument();
  });

  it("lists what Nocturne learned from it and can delete it", async () => {
    await open("cal-leo-swim");
    expect(
      screen.getByText("Leo, swimming at Riverside pool, Friday 13 November at 4:30 pm."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete…" })).toBeEnabled();
  });

  it("goes back to what the phone brought, and offers Mark private and Delete only", async () => {
    await open("cal-dentist");
    expect(screen.getByRole("link", { name: /Calendar from Sam’s phone/ })).toHaveAttribute(
      "href",
      "#/settings/phones/came-in/pixel/calendar",
    );
    const buttons = screen.getAllByRole("button").map((b) => b.textContent);
    expect(buttons).toEqual(expect.arrayContaining(["Mark private", "Delete…"]));
    expect(buttons).not.toContain("Keep forever");
    expect(buttons).not.toContain("Read it again");
  });

  it("says an entry gone from the calendar is removed, and keeps it", async () => {
    await open("cal-team-lunch");
    expect(screen.getByText("Removed from your calendar")).toBeInTheDocument();
    expect(screen.queryByText(/last updated/)).toBeNull();
  });

  it("shows an all-day entry as all day", async () => {
    await open("cal-inset-day");
    const page = screen.getByRole("main");
    expect(within(page).getAllByText(/all day/).length).toBeGreaterThan(0);
  });
});
