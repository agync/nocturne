/** The speech model reads 16 kHz mono 16-bit sound in a WAV file, and not much else. */
export const TARGET_RATE = 16000;

const HEADER_BYTES = 44;

/** A WAV file (RIFF/WAVE, 16 kHz mono 16-bit PCM) of the samples, in order. */
export function encodeWav(chunks: readonly Int16Array[]): Uint8Array {
  const samples = chunks.reduce((n, chunk) => n + chunk.length, 0);
  const data = samples * 2;
  const out = new Uint8Array(HEADER_BYTES + data);
  const view = new DataView(out.buffer);
  const ascii = (at: number, word: string) => {
    for (let i = 0; i < word.length; i += 1) view.setUint8(at + i, word.charCodeAt(i));
  };
  ascii(0, "RIFF");
  view.setUint32(4, 36 + data, true);
  ascii(8, "WAVE");
  ascii(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, TARGET_RATE, true);
  view.setUint32(28, TARGET_RATE * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  ascii(36, "data");
  view.setUint32(40, data, true);
  let at = HEADER_BYTES;
  for (const chunk of chunks) {
    for (const sample of chunk) {
      view.setInt16(at, sample, true);
      at += 2;
    }
  }
  return out;
}

/** Float samples in -1..1 as 16-bit, clipped at the ends. */
export function toInt16(samples: Float32Array): Int16Array {
  const out = new Int16Array(samples.length);
  for (let i = 0; i < samples.length; i += 1) {
    const x = Math.max(-1, Math.min(1, samples[i] as number));
    out[i] = x < 0 ? Math.round(x * 32768) : Math.round(x * 32767);
  }
  return out;
}

/**
 * Turns sound at one rate into 16 kHz a buffer at a time. Each output sample is the average of
 * the input it spans, which keeps higher pitches from folding down into the speech. The result
 * does not depend on how the sound is cut into buffers.
 */
export class Resampler {
  private readonly ratio: number;
  private need: number;
  private sum = 0;

  constructor(inputRate: number) {
    this.ratio = inputRate / TARGET_RATE;
    this.need = this.ratio;
  }

  push(input: Float32Array, emit: (sample: number) => void): void {
    for (const x of input) {
      let left = 1;
      while (left > 1e-9) {
        const take = Math.min(left, this.need);
        this.sum += x * take;
        this.need -= take;
        left -= take;
        if (this.need <= 1e-9) {
          emit(this.sum / this.ratio);
          this.sum = 0;
          this.need = this.ratio;
        }
      }
    }
  }
}
