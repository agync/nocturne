import { encodeWav, Resampler, TARGET_RATE, toInt16 } from "./wav";

/** The longest recording kept in one piece: an hour is 115 MB of sound, well inside the limit. */
export const MAX_RECORDING_SECONDS = 60 * 60;

type AudioContextClass = typeof AudioContext;

function contextClass(): AudioContextClass | undefined {
  if (typeof window === "undefined") return undefined;
  return (
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: AudioContextClass }).webkitAudioContext
  );
}

export function canCaptureSound(): boolean {
  return contextClass() !== undefined;
}

export interface WavCapture {
  /** For a level meter. Null when the browser cannot make one. */
  analyser: AnalyserNode | null;
  /** Stops listening and returns the sound as a WAV file, or null when there was none. */
  finish(): Uint8Array | null;
  /** Stops listening and lets the sound go. */
  discard(): void;
}

/**
 * Listens to the microphone through the Web Audio API and keeps what it hears as 16 kHz mono
 * 16-bit sound, so the kept file is a WAV the speech model can read whatever the web view can
 * record natively. Memory is one 16-bit number per sample; `onLimit` fires once at the cap and
 * nothing more is kept after it.
 */
export function startWavCapture(
  media: MediaStream,
  onLimit: () => void,
  maxSeconds = MAX_RECORDING_SECONDS,
): WavCapture {
  const Context = contextClass();
  if (!Context) throw new Error("no audio context");
  const context = new Context();
  void context.resume?.().catch(() => undefined);

  const source = context.createMediaStreamSource(media);
  let analyser: AnalyserNode | null = null;
  try {
    analyser = context.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);
  } catch {
    analyser = null;
  }

  // A script processor is the one tap on the sound every web view has. It must reach the output
  // to run, so it goes through a gain of zero: nothing is played back.
  const processor = context.createScriptProcessor(4096, 1, 1);
  const mute = context.createGain();
  mute.gain.value = 0;
  source.connect(processor);
  processor.connect(mute);
  mute.connect(context.destination);

  const resampler = new Resampler(context.sampleRate);
  const chunks: Int16Array[] = [];
  const limit = maxSeconds * TARGET_RATE;
  let total = 0;
  let open = true;
  let limited = false;

  processor.onaudioprocess = (event) => {
    if (!open || limited) return;
    const buffer = event.inputBuffer;
    const mono = downmix(buffer);
    const out: number[] = [];
    resampler.push(mono, (x) => out.push(x));
    const room = limit - total;
    const kept = out.length > room ? out.slice(0, room) : out;
    if (kept.length > 0) {
      chunks.push(toInt16(Float32Array.from(kept)));
      total += kept.length;
    }
    if (total >= limit) {
      limited = true;
      onLimit();
    }
  };

  const close = () => {
    open = false;
    processor.onaudioprocess = null;
    for (const node of [processor, mute, source]) {
      try {
        node.disconnect();
      } catch {
        // Already apart.
      }
    }
    void context.close().catch(() => undefined);
  };

  return {
    analyser,
    finish() {
      close();
      if (total === 0) return null;
      const file = encodeWav(chunks);
      chunks.length = 0;
      return file;
    },
    discard() {
      close();
      chunks.length = 0;
    },
  };
}

function downmix(buffer: AudioBuffer): Float32Array {
  const channels = buffer.numberOfChannels;
  if (channels <= 1) return buffer.getChannelData(0);
  const mono = new Float32Array(buffer.length);
  for (let c = 0; c < channels; c += 1) {
    const data = buffer.getChannelData(c);
    for (let i = 0; i < mono.length; i += 1)
      mono[i] = (mono[i] as number) + (data[i] as number) / channels;
  }
  return mono;
}
