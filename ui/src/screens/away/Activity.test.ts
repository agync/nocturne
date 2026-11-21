import { describe, expect, it } from "vitest";
import type { RemoteEvent } from "../../engine/bindings";
import { eventText } from "./Activity";

const event = (count: number): RemoteEvent =>
  ({
    at: "2026-11-19T10:00:00Z",
    deviceName: "Sam’s phone",
    action: "answeredName",
    result: "ok",
    count,
  }) as RemoteEvent;

describe("the activity line for a same-name answer", () => {
  it("names the badge's words, once and many times", () => {
    expect(eventText(event(1))).toBe("Sam’s phone answered a same-name question");
    expect(eventText(event(3))).toBe("Sam’s phone answered 3 same-name questions");
  });
});

describe("the activity lines for what phones send", () => {
  it("says how many changes and readings, never what they were", () => {
    const sent = (action: "sentCalendar" | "sentReadings", count: number) =>
      ({ ...event(count), action }) as RemoteEvent;
    expect(eventText(sent("sentCalendar", 1))).toBe("Sam’s phone sent 1 calendar change");
    expect(eventText(sent("sentCalendar", 4))).toBe("Sam’s phone sent 4 calendar changes");
    expect(eventText(sent("sentReadings", 1))).toBe("Sam’s phone sent 1 reading");
    expect(eventText(sent("sentReadings", 12))).toBe("Sam’s phone sent 12 readings");
  });
});
