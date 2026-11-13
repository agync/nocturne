import { describe, expect, it } from "vitest";
import { MEDIUM_PX, NARROW_PX, shellSize } from "./breakpoints";

describe("shellSize", () => {
  it("uses the 720 px breakpoint at normal text size", () => {
    expect(NARROW_PX).toBe(720);
    expect(shellSize(719, 16)).toBe("narrow");
    expect(shellSize(720, 16)).toBe("medium");
    expect(shellSize(MEDIUM_PX, 16)).toBe("wide");
  });

  it("reflows sooner when the text is larger", () => {
    expect(shellSize(1280, 32)).toBe("narrow");
    expect(shellSize(1280, 24)).toBe("medium");
  });
});
