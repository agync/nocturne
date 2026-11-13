import { describe, expect, it } from "vitest";

describe("toolchain", () => {
  it("runs in the fixed zone and locale", () => {
    expect(1 + 1).toBe(2);
    expect(process.env.TZ).toBe("Europe/London");
    expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe("Europe/London");
  });
});
