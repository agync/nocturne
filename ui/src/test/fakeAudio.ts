import { vi } from "vitest";

interface FakeNode {
  connect: () => void;
  disconnect: () => void;
}

/** Just enough of the Web Audio API for the recorder, with sound fed in by `speak`. */
export class FakeAudioContext {
  static last: FakeAudioContext | null = null;
  static rate = 48000;
  /** When set, some sound arrives as soon as the recorder starts listening. */
  static autoSpeak = false;
  sampleRate = FakeAudioContext.rate;
  state = "running";
  closed = false;
  destination = {};
  processor: (FakeNode & { onaudioprocess: ((event: unknown) => void) | null }) | null = null;

  constructor() {
    FakeAudioContext.last = this;
  }

  createMediaStreamSource(): FakeNode {
    return { connect: () => undefined, disconnect: () => undefined };
  }

  createAnalyser() {
    return {
      fftSize: 0,
      getByteTimeDomainData: (samples: Uint8Array) => samples.fill(128),
      connect: () => undefined,
      disconnect: () => undefined,
    };
  }

  createGain() {
    return { gain: { value: 1 }, connect: () => undefined, disconnect: () => undefined };
  }

  createScriptProcessor() {
    this.processor = {
      onaudioprocess: null,
      connect: () => undefined,
      disconnect: () => undefined,
    };
    if (FakeAudioContext.autoSpeak) {
      queueMicrotask(() => {
        if (this.processor?.onaudioprocess) speak(200);
      });
    }
    return this.processor;
  }

  resume() {
    return Promise.resolve();
  }

  close() {
    this.closed = true;
    return Promise.resolve();
  }
}

export function installFakeAudio(rate = 48000, autoSpeak = false): void {
  FakeAudioContext.rate = rate;
  FakeAudioContext.autoSpeak = autoSpeak;
  FakeAudioContext.last = null;
  vi.stubGlobal("AudioContext", FakeAudioContext);
}

/** Feeds `ms` of a steady tone to the open recording, a buffer at a time, as the browser would. */
export function speak(ms = 500, level = 0.2): void {
  const context = FakeAudioContext.last;
  const processor = context?.processor;
  if (!context || !processor?.onaudioprocess) throw new Error("nothing is listening");
  let left = Math.round((context.sampleRate * ms) / 1000);
  while (left > 0) {
    const length = Math.min(4096, left);
    const data = new Float32Array(length).fill(level);
    processor.onaudioprocess({
      inputBuffer: { numberOfChannels: 1, length, getChannelData: () => data },
    });
    left -= length;
  }
}
