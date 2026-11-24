import { describe, expect, it } from "vitest";
import { afterLook, against, marked, matching, sortNames, typingHelps } from "./modelList";

describe("the service's list", () => {
  it("runs A to Z ignoring capitals, with numbers in number order", () => {
    expect(
      sortNames(["reader-10", "Writer", "reader-2", "alpha", "reader-1", "Reader-3", "alpha"]),
    ).toEqual(["alpha", "reader-1", "reader-2", "Reader-3", "reader-10", "Writer"]);
  });

  it("finds any part of a name, ignoring capitals, and marks it", () => {
    const names = ["open-reader-SMALL", "reader-large", "reader-small-2026-05"];
    expect(matching(names, "small")).toEqual(["open-reader-SMALL", "reader-small-2026-05"]);
    expect(matching(names, "  ")).toEqual(names);
    expect(marked("open-reader-SMALL", "small")).toEqual([
      { text: "open-reader-", match: false },
      { text: "SMALL", match: true },
    ]);
    expect(marked("aXaXa", "x")).toEqual([
      { text: "a", match: false },
      { text: "X", match: true },
      { text: "a", match: false },
      { text: "X", match: true },
      { text: "a", match: false },
    ]);
  });

  it("says where the typed name stands against the list", () => {
    const names = ["reader-small", "Reader-Large"];
    expect(against("reader-small", names)).toEqual({ kind: "on" });
    expect(against("reader-smal", names)).toEqual({ kind: "notOn" });
    expect(against("reader-large", names)).toEqual({ kind: "capitals", listed: "Reader-Large" });
    expect(against("reader-small", null)).toEqual({ kind: "none" });
    expect(against("", names)).toEqual({ kind: "none" });
  });

  it("tells failures that typing can help from those it can't", () => {
    expect(typingHelps("busy")).toBe(true);
    expect(typingHelps("serverError")).toBe(true);
    expect(typingHelps("keyRefused")).toBe(false);
    expect(typingHelps("noInternet")).toBe(false);
  });

  it("turns a look into the list's state", () => {
    expect(afterLook({ type: "listed", names: ["b", "A"] })).toEqual({
      name: "listed",
      names: ["A", "b"],
    });
    expect(afterLook({ type: "none" })).toEqual({ name: "none" });
    expect(afterLook({ type: "failed", problem: { type: "keyRefused" } })).toEqual({
      name: "failed",
      problem: "keyRefused",
    });
  });
});
