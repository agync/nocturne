import { describe, expect, it } from "vitest";
import { keptHint } from "../../screens/Original";

// Where a thing was kept, once a memory has moved: the device's name when it travelled with the
// thing, and the day and time alone when no name came with it.
const AT = "2026-11-20T07:05:00Z";

describe("the Kept row after a move", () => {
  it("names the device a thing was kept on", () => {
    expect(keptHint({ how: null, device: "Ana’s Pixel", workedOut: false }, AT)).toMatch(
      /^Kept on Ana’s Pixel · Friday 20 November, 7:05\s?am$/,
    );
  });

  it("says the day and time alone when no name came with it", () => {
    expect(keptHint({ how: null, device: null, workedOut: false }, AT)).toMatch(
      /^Friday 20 November, 7:05\s?am$/,
    );
  });

  it("keeps how it was kept here", () => {
    expect(keptHint({ how: "typed", device: null, workedOut: false }, AT)).toMatch(
      /^Typed on this phone · /,
    );
  });
});
