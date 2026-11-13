import { describe, expect, it } from "vitest";
import { topicOf } from "./topic";

describe("topicOf", () => {
  it.each([
    ["What did I promise the bakery?", "bakery"],
    ["When is the dentist?", "dentist"],
    ["When does my passport run out?", "passport"],
    ["passport", "passport"],
    ["Where is Gran’s house", "house"],
  ])("%s is about %s", (question, topic) => {
    expect(topicOf(question)).toBe(topic);
  });

  it("falls back to the question itself when nothing is left", () => {
    expect(topicOf("What is it?")).toBe("What is it");
  });
});
