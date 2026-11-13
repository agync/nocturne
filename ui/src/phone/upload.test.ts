// @vitest-environment node
import { describe, expect, it } from "vitest";
import * as P from "./protocol";
import { sendBlob, type UploadTrace } from "./upload";

describe("upload", () => {
  it("holds at most two chunks of a large file at once, and sends them in order", async () => {
    const blob = new Blob([new Uint8Array(P.CHUNK * 9 + 5)]);
    const trace: UploadTrace = { held: 0, maxHeld: 0 };
    const seen: Array<[number, boolean, number]> = [];
    await sendBlob(
      {
        async chunk(seq, last, data) {
          await new Promise((r) => setTimeout(r, 2));
          seen.push([seq, last, data.length]);
        },
      },
      blob,
      () => false,
      trace,
    );
    expect(trace.maxHeld).toBeLessThanOrEqual(2);
    expect(trace.held).toBe(0);
    expect(seen.map((s) => s[0])).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(seen.filter((s) => s[1])).toEqual([[9, true, 5]]);
    expect(seen.slice(0, 9).every((s) => s[2] === P.CHUNK)).toBe(true);
  });

  it("stops quietly when the call has already ended", async () => {
    const blob = new Blob([new Uint8Array(P.CHUNK * 4)]);
    let sent = 0;
    await sendBlob(
      {
        async chunk() {
          sent += 1;
        },
      },
      blob,
      () => sent >= 1,
    );
    expect(sent).toBe(1);
  });

  it("a single small chunk is also the last", async () => {
    const got: boolean[] = [];
    await sendBlob(
      { chunk: async (_s, last) => void got.push(last) },
      new Blob([new Uint8Array(3)]),
      () => false,
    );
    expect(got).toEqual([true]);
  });
});
