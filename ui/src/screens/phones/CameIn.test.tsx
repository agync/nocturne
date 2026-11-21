import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CameInRow } from "../../engine/bindings";
import { NOW, openPhones, phonesEngine } from "./testing";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(NOW));
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.location.hash = "";
});

const PATH = "/settings/phones/came-in/pixel/calendar";

describe("what came in", () => {
  it("lists the newest change first with its tag and day", async () => {
    await openPhones(PATH, "Calendar from Sam’s phone");
    expect(
      screen.getByText("Newest change first. Each one opens its own page."),
    ).toBeInTheDocument();
    const list = within(await screen.findByRole("list", { name: "What came in" }));
    const rows = list.getAllByRole("listitem");
    expect(rows).toHaveLength(6);
    const first = within(rows[0] as HTMLElement);
    expect(first.getByText("Today 7:42 am")).toBeInTheDocument();
    expect(first.getByText("changed")).toBeInTheDocument();
    expect(
      within(rows[2] as HTMLElement).getByText("removed from your calendar"),
    ).toBeInTheDocument();
    expect(within(rows[3] as HTMLElement).getByText(/^Yesterday 9:10 pm$/)).toBeInTheDocument();
    expect(within(rows[5] as HTMLElement).getByText(/7:58 am$/)).toBeInTheDocument();
    expect(first.getByRole("link")).toHaveAttribute("href", expect.stringMatching(/^#\/items\/.+/));
    expect(screen.getByRole("link", { name: "From your phones" })).toHaveAttribute(
      "href",
      "#/settings/phones",
    );
  });

  it("shows the entry's time and calendar when the computer says them, and its day when not", async () => {
    await openPhones(PATH, "Calendar from Sam’s phone");
    expect(await screen.findByText(/, 4:30 pm · Family$/)).toBeInTheDocument();
    expect(screen.getByText(/, all day · School term dates$/)).toBeInTheDocument();
    const engine = phonesEngine();
    const rows = engine.sim.fixtures.cameIn["pixel/calendar"] as CameInRow[];
    engine.sim.fixtures.cameIn["pixel/calendar"] = rows.slice(0, 1).map((row) => ({
      itemId: row.itemId,
      cameInAt: row.cameInAt,
      title: "Bare entry",
      day: "2026-11-20",
      tag: row.tag,
    }));
    cleanup();
    await openPhones(PATH, "Calendar from Sam’s phone", engine);
    expect(await screen.findByText("Friday 20 November")).toBeInTheDocument();
  });

  it("pages thirty at a time with a quiet load more", async () => {
    const engine = phonesEngine();
    const base = (engine.sim.fixtures.cameIn["pixel/calendar"] as CameInRow[])[0] as CameInRow;
    const many: CameInRow[] = Array.from({ length: 35 }, (_, i) => ({
      ...base,
      itemId: `item-${i}`,
      title: `Entry ${i}`,
      cameInAt: new Date(Date.parse(NOW) - i * 60_000).toISOString(),
    }));
    engine.sim.fixtures.cameIn["pixel/calendar"] = many;
    const list = vi.spyOn(engine, "listCameIn");
    await openPhones(PATH, "Calendar from Sam’s phone", engine);
    expect(await screen.findAllByRole("listitem")).toHaveLength(30);
    expect(list).toHaveBeenCalledWith("pixel", "calendar", null, 30, null);
    await userEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(35));
    expect(list).toHaveBeenLastCalledWith(
      "pixel",
      "calendar",
      many[29]?.cameInAt,
      30,
      many[29]?.itemId,
    );
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("pages past rows that all came in at the same time", async () => {
    const engine = phonesEngine();
    const base = (engine.sim.fixtures.cameIn["pixel/calendar"] as CameInRow[])[0] as CameInRow;
    engine.sim.fixtures.cameIn["pixel/calendar"] = Array.from({ length: 35 }, (_, i) => ({
      ...base,
      itemId: `item-${String(i).padStart(2, "0")}`,
      title: `Entry ${i}`,
      cameInAt: base.cameInAt,
    }));
    await openPhones(PATH, "Calendar from Sam’s phone", engine);
    expect(await screen.findAllByRole("listitem")).toHaveLength(30);
    await userEvent.click(screen.getByRole("button", { name: "Load more" }));
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(35));
    expect(
      new Set(screen.getAllByRole("link").map((a) => a.getAttribute("href"))).size,
    ).toBeGreaterThan(34);
  });

  it("says so when nothing came in", async () => {
    const engine = phonesEngine();
    engine.sim.fixtures.cameIn["pixel/calendar"] = [];
    await openPhones(PATH, "Calendar from Sam’s phone", engine);
    expect(
      await screen.findByText("Nothing has come in from Sam’s phone yet."),
    ).toBeInTheDocument();
  });
});
