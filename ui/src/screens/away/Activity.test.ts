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
