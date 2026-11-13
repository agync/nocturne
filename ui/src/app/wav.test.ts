import { describe, expect, it } from "vitest";
import { encodeWav, Resampler, TARGET_RATE, toInt16 } from "./wav";

const text = (view: DataView, at: number, length: number) =>
  String.fromCharCode(...Array.from({ length }, (_, i) => view.getUint8(at + i)));

describe("encodeWav", () => {
  it("writes a RIFF/WAVE header for 16 kHz mono 16-bit with the right sizes", () => {
    const samples = Int16Array.from({ length: 1600 }, (_, i) =>
      Math.round(Math.sin(i / 10) * 20000),
    );
    const bytes = encodeWav([samples.subarray(0, 1000), samples.subarray(1000)]);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    expect(bytes.length).toBe(44 + 3200);
    expect(text(view, 0, 4)).toBe("RIFF");
    expect(view.getUint32(4, true)).toBe(36 + 3200);
    expect(text(view, 8, 4)).toBe("WAVE");
    expect(text(view, 12, 4)).toBe("fmt ");
    expect(view.getUint32(16, true)).toBe(16);
    expect(view.getUint16(20, true)).toBe(1);
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getUint32(24, true)).toBe(16000);
    expect(view.getUint32(28, true)).toBe(32000);
    expect(view.getUint16(32, true)).toBe(2);
    expect(view.getUint16(34, true)).toBe(16);
    expect(text(view, 36, 4)).toBe("data");
    expect(view.getUint32(40, true)).toBe(3200);
  });

  it("carries the samples unchanged, in order across chunks", () => {
    const a = Int16Array.from([1, -2, 3]);
    const b = Int16Array.from([32767, -32768]);
    const bytes = encodeWav([a, b]);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const read = Array.from({ length: 5 }, (_, i) => view.getInt16(44 + i * 2, true));
    expect(read).toEqual([1, -2, 3, 32767, -32768]);
  });

  it("is a valid empty file when nothing was recorded", () => {
    const bytes = encodeWav([]);
    expect(bytes.length).toBe(44);
    expect(new DataView(bytes.buffer).getUint32(40, true)).toBe(0);
  });
});

describe("toInt16", () => {
  it("scales and clips", () => {
    expect(Array.from(toInt16(Float32Array.from([0, 1, -1, 2, -2, 0.5])))).toEqual([
      0, 32767, -32768, 32767, -32768, 16384,
    ]);
  });
});

describe("Resampler", () => {
  const run = (rate: number, input: Float32Array, split = input.length) => {
    const out: number[] = [];
    const r = new Resampler(rate);
    for (let at = 0; at < input.length; at += split) {
      r.push(input.subarray(at, at + split), (x) => out.push(x));
    }
    return out;
  };

  it("turns 48 kHz into 16 kHz: one third as many samples, a steady level stays steady", () => {
    const out = run(48000, new Float32Array(48000).fill(0.5));
    expect(out.length).toBeGreaterThanOrEqual(15999);
    expect(out.length).toBeLessThanOrEqual(16000);
    for (const x of out) expect(x).toBeCloseTo(0.5, 5);
  });

  it("handles 44.1 kHz, where the ratio is not whole", () => {
    const out = run(44100, new Float32Array(44100).fill(-0.25));
    expect(Math.abs(out.length - TARGET_RATE)).toBeLessThanOrEqual(1);
    for (const x of out) expect(x).toBeCloseTo(-0.25, 5);
  });

  it("gives the same result however the sound is cut into buffers", () => {
    const signal = Float32Array.from({ length: 44100 }, (_, i) => Math.sin(i / 20));
    const whole = run(44100, signal);
    const cut = run(44100, signal, 4096);
    expect(cut.length).toBe(whole.length);
    for (const [i, x] of cut.entries()) expect(x).toBeCloseTo(whole[i] as number, 5);
  });

  it("keeps a tone at its pitch: 440 Hz stays 440 Hz at 16 kHz", () => {
    const rate = 48000;
    const signal = Float32Array.from({ length: rate }, (_, i) =>
      Math.sin((2 * Math.PI * 440 * i) / rate),
    );
    const out = run(rate, signal);
    let crossings = 0;
    for (let i = 1; i < out.length; i += 1) {
      if ((out[i - 1] as number) < 0 && (out[i] as number) >= 0) crossings += 1;
    }
    expect(Math.abs(crossings - 440)).toBeLessThanOrEqual(2);
  });

  it("passes a lower rate through by repeating samples", () => {
    const out = run(8000, Float32Array.from([1, -1]));
    expect(out).toEqual([1, 1, -1, -1]);
  });
});
