import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Receipt } from "../../engine/bindings";
import { calendarMeta, receiptMeta, sleepMeta } from "../labels";
import { createMockPhoneEngine } from "../mock";
import { memoryQueue } from "../queue";
import { todayDay } from "../testing/journalFixtures";
import { HEALTH, withCalendar, withPhones } from "../testing/phoneFixtures";
import { CheckIn } from "./CheckIn";
import { Journal } from "./Journal";
import { PageScreen } from "./PageScreen";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the journal and the check-in with what the phone brought", () => {
  const noop = () => undefined;

  it("puts the night, the walk and appointments on the timeline, then From Health", async () => {
    const opened: string[] = [];
    const engine = createMockPhoneEngine();
    await engine.connect();
    engine.getJournalDay = async () => withPhones(todayDay());
    render(
      <Journal
        engine={engine}
        queue={memoryQueue()}
        connection={engine.connection()}
        computerName="Home computer"
        day={null}
        today={null}
        onBack={noop}
        onDay={(day) => opened.push(day)}
        onOpen={noop}
        onWhich={noop}
        onReflect={noop}
        onReflection={noop}
        onKeep={noop}
        onCheckIn={noop}
        phone
      />,
    );
    expect(await screen.findByText("Woke · 6 h 40 min asleep")).toBeInTheDocument();
    expect(screen.getAllByText("from your calendar")).toHaveLength(2);
    expect(screen.getByText("Walk · 42 min · 3.1 km")).toBeInTheDocument();
    expect(screen.getByText("after 6 h 40 min asleep")).toBeInTheDocument();
    const health = screen.getByRole("region", { name: "From Health" });
    expect(within(health).getByText("8,412 today")).toBeInTheDocument();
    expect(within(health).getByText("52 to 148 · mostly around 74")).toBeInTheDocument();
    expect(within(health).getAllByRole("button", { name: "Show as a list" })).toHaveLength(2);
    fireEvent.click(
      within(health).getAllByRole("button", { name: "Show as a list" })[1] as Element,
    );
    fireEvent.click(within(health).getByRole("button", { name: "Thu 19" }));
    expect(opened).toEqual(["2026-11-19"]);
  });

  it("shows last night above the check-in once it has come in", async () => {
    const engine = createMockPhoneEngine();
    await engine.connect();
    engine.getJournalDay = async () => ({ ...todayDay(), health: HEALTH });
    render(
      <CheckIn
        engine={engine}
        queue={memoryQueue()}
        connection={engine.connection()}
        deviceName="Pixel"
        now={() => 0}
        onBack={noop}
        onQueued={noop}
        lastNight
      />,
    );
    expect(await screen.findByText("Last night: 6 h 40 min asleep")).toBeInTheDocument();
  });

  it("leaves Health out of the phone page in a browser (decision 4)", async () => {
    const engine = createMockPhoneEngine();
    await engine.connect();
    engine.getJournalDay = async () => withPhones(todayDay());
    render(
      <Journal
        engine={engine}
        queue={memoryQueue()}
        connection={engine.connection()}
        computerName="Home computer"
        day={null}
        today={null}
        onBack={noop}
        onDay={noop}
        onOpen={noop}
        onWhich={noop}
        onReflect={noop}
        onReflection={noop}
        onKeep={noop}
        onCheckIn={noop}
      />,
    );
    expect(await screen.findByText("Dentist, check-up")).toBeInTheDocument();
    expect(screen.queryByText("from your calendar")).toBeNull();
    expect(screen.queryByText(/Woke/)).toBeNull();
    expect(screen.queryByText(/Walk/)).toBeNull();
    expect(screen.queryByText(/after .* asleep/)).toBeNull();
    expect(screen.queryByRole("region", { name: "From Health" })).toBeNull();
  });

  it("asks for no journal day on the check-in in a browser", async () => {
    const engine = createMockPhoneEngine();
    await engine.connect();
    const day = vi.spyOn(engine, "getJournalDay");
    render(
      <CheckIn
        engine={engine}
        queue={memoryQueue()}
        connection={engine.connection()}
        deviceName="Pixel"
        now={() => 0}
        onBack={noop}
        onQueued={noop}
      />,
    );
    await screen.findByText("How are you feeling?");
    expect(day).not.toHaveBeenCalled();
  });
});

describe("the page in a browser", () => {
  it("leaves out last night and the calendar (decision 4)", async () => {
    const engine = createMockPhoneEngine();
    await engine.connect();
    const page = engine.getPage.bind(engine);
    engine.getPage = async (date) => withCalendar(await page(date));
    render(
      <PageScreen
        engine={engine}
        queue={memoryQueue()}
        connected
        now={() => 0}
        onQueued={vi.fn()}
        onOpen={vi.fn()}
        onJournal={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(await screen.findByText("Two things need you today.")).toBeInTheDocument();
    expect(screen.queryByText(/In your calendar today/)).toBeNull();
    expect(screen.queryByText("Last night")).toBeNull();
  });
});

describe("receipts from a phone", () => {
  it("names an appointment by its calendar and a night by its phone", () => {
    expect(calendarMeta("Family")).toBe("Calendar · Family");
    expect(sleepMeta("Pixel")).toBe("Sleep · Health · Pixel");
    const receipt = {
      itemId: "r",
      kind: "reading",
      source: "phone",
      label: "",
      keptAt: "2026-11-19T07:05:00Z",
      durationMs: null,
      span: null,
      unconfirmed: false,
      fromDevice: "Pixel",
    } satisfies Receipt;
    expect(receiptMeta(receipt)).toBe("Health · Pixel");
    expect(receiptMeta({ ...receipt, label: "Steps" })).toBe("Steps · Health · Pixel");
    expect(receiptMeta({ ...receipt, kind: "calendar", calendar: "Family" })).toBe(
      "Calendar · Family · Pixel",
    );
  });
});
