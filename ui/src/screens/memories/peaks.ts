import { useEffect, useState } from "react";

/** The loudest sample in each of `count` equal slices, scaled so the loudest slice is 1. */
export function computePeaks(samples: Float32Array, count: number): number[] {
  if (samples.length === 0 || count <= 0) return [];
  const size = Math.max(1, Math.floor(samples.length / count));
  const peaks: number[] = [];
  for (let i = 0; i < count; i += 1) {
    let max = 0;
    const end = Math.min(samples.length, (i + 1) * size);
    for (let j = i * size; j < end; j += 1) max = Math.max(max, Math.abs(samples[j] ?? 0));
    peaks.push(max);
  }
  const loudest = Math.max(...peaks);
  return loudest > 0 ? peaks.map((p) => p / loudest) : peaks.map(() => 0);
}

/**
 * Decodes the original in the browser to draw its waveform. Where that is not possible (no audio
 * support, a picture standing in for a file, a blocked request) it gives null and the screen
 * draws even bars: the bars then say nothing about the sound, which the screen does not claim.
 */
export function usePeaks(src: string | null, count = 72): number[] | null {
  const [peaks, setPeaks] = useState<number[] | null>(null);
  useEffect(() => {
    setPeaks(null);
    if (!src || src.startsWith("data:") || typeof AudioContext === "undefined") return;
    let live = true;
    (async () => {
      try {
        const bytes = await (await fetch(src)).arrayBuffer();
        const context = new AudioContext();
        const audio = await context.decodeAudioData(bytes);
        void context.close();
        if (live) setPeaks(computePeaks(audio.getChannelData(0), count));
      } catch {
        // Even bars it is.
      }
    })();
    return () => {
      live = false;
    };
  }, [src, count]);
  return peaks;
}
