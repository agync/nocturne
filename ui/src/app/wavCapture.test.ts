import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeAudioContext, installFakeAudio, speak } from "../test/fakeAudio";
import { startWavCapture } from "./wavCapture";

const media = {} as MediaStream;

beforeEach(() => installFakeAudio(48000));
afterEach(() => vi.unstubAllGlobals());

const samplesOf = (file: Uint8Array) => {
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  return Array.from({ length: (file.length - 44) / 2 }, (_, i) => view.getInt16(44 + i * 2, true));
};

describe("startWavCapture", () => {
  it("keeps what it hears as a 16 kHz mono WAV", () => {
    const capture = startWavCapture(media, vi.fn());
    speak(1000, 0.5);
    const file = capture.finish();
    if (!file) throw new Error("expected a file");
    const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
    expect(String.fromCharCode(...file.subarray(0, 4))).toBe("RIFF");
    expect(view.getUint32(24, true)).toBe(16000);
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getUint16(34, true)).toBe(16);
    const samples = samplesOf(file);
    expect(Math.abs(samples.length - 16000)).toBeLessThanOrEqual(1);
    expect(samples.at(100)).toBe(Math.round(0.5 * 32767));
    expect(view.getUint32(40, true)).toBe(samples.length * 2);
    expect(view.getUint32(4, true)).toBe(36 + samples.length * 2);
  });

  it("gives nothing when no sound arrived, and closes the audio", () => {
    const capture = startWavCapture(media, vi.fn());
    expect(capture.finish()).toBeNull();
    expect(FakeAudioContext.last?.closed).toBe(true);
  });

  it("lets the sound go on discard", () => {
    const capture = startWavCapture(media, vi.fn());
    speak(200);
    capture.discard();
    expect(FakeAudioContext.last?.closed).toBe(true);
  });

  it("mixes two channels down to one", () => {
    const capture = startWavCapture(media, vi.fn());
    const left = new Float32Array(4096).fill(0.8);
    const right = new Float32Array(4096).fill(0.2);
    FakeAudioContext.last?.processor?.onaudioprocess?.({
      inputBuffer: {
        numberOfChannels: 2,
        length: 4096,
        getChannelData: (c: number) => (c === 0 ? left : right),
      },
    });
    const file = capture.finish();
    if (!file) throw new Error("expected a file");
    expect(samplesOf(file)[10]).toBe(Math.round(0.5 * 32767));
  });

  it("stops keeping at the limit, says so once and still hands over what it has", () => {
    const onLimit = vi.fn();
    const capture = startWavCapture(media, onLimit, 1);
    speak(3000);
    expect(onLimit).toHaveBeenCalledOnce();
    const file = capture.finish();
    if (!file) throw new Error("expected a file");
    expect(samplesOf(file).length).toBe(16000);
  });

  it("works through the older prefixed name", () => {
    vi.unstubAllGlobals();
    vi.stubGlobal("AudioContext", undefined);
    vi.stubGlobal("webkitAudioContext", FakeAudioContext);
    FakeAudioContext.last = null;
    const capture = startWavCapture(media, vi.fn());
    speak(100);
    expect(capture.finish()).not.toBeNull();
  });
});
