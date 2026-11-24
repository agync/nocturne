import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../home.css"), "utf8");

/** The rule a selector's block holds in the stylesheet. */
function block(selector: string): string {
  const at = css.indexOf(`${selector} {`);
  if (at < 0) throw new Error(`no rule for ${selector}`);
  return css.slice(at, css.indexOf("}", at));
}

describe("sizes in What was sent", () => {
  it("are written one way, as the size formatter writes them, in a part's heading too", () => {
    // The row's meta reads "9.9 kB"; a part's heading must not turn its size into "1.9 KB".
    expect(block(".ph-mind-part-h")).not.toMatch(/text-transform/);
    expect(block(".ph-mind-sent-meta")).not.toMatch(/text-transform/);
  });
});
