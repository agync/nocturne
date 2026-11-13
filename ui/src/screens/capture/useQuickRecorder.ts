import { useCallback, useEffect, useRef, useState } from "react";
import { canRecord, type RecorderFailure } from "../../app/useRecorder";

/** A press of Space shorter than this was a slip of the finger, not a voice note. */
export const MIN_HOLD_MS = 300;

/** How many bars the level meter draws. */
export const METER_BARS = 36;

const DENIED = new Set([
  "NotAllowedError",
  "SecurityError",
  "PermissionDeniedError",
  "NotFoundError",
]);

interface Options {
  onFinished: (bytes: Uint8Array, mime: string, durationMs: number) => void;
  onFailed: (reason: RecorderFailure) => void;
}

const quiet = () => new Array<number>(METER_BARS).fill(0.08);

/**
 * Records while a key is held or until the person says so, with a live level for the meter.
 * Stopping with `keep: false` throws the sound away; nothing leaves the computer either way.
 */
export function useQuickRecorder({ onFinished, onFailed }: Options) {
  const [recording, setRecording] = useState(false);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [levels, setLevels] = useState<number[]>(quiet);
  const recorder = useRef<MediaRecorder | null>(null);
  const stream = useRef<MediaStream | null>(null);
  const audio = useRef<AudioContext | null>(null);
  const chunks = useRef<Blob[]>([]);
  const startedAt = useRef(0);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const starting = useRef(false);
  const stopWanted = useRef<null | { keep: boolean }>(null);
  const disposed = useRef(false);
  const keep = useRef(true);
  const latest = useRef({ onFinished, onFailed });
  latest.current = { onFinished, onFailed };

  const release = useCallback(() => {
    if (timer.current) clearInterval(timer.current);
    timer.current = null;
    for (const track of stream.current?.getTracks() ?? []) track.stop();
    stream.current = null;
    void audio.current?.close().catch(() => undefined);
    audio.current = null;
    recorder.current = null;
    setRecording(false);
    setElapsedMs(0);
    setLevels(quiet());
  }, []);

  const start = useCallback(async () => {
    if (recorder.current || starting.current) return;
    if (!canRecord()) {
      latest.current.onFailed("unsupported");
      return;
    }
    starting.current = true;
    stopWanted.current = null;
    try {
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (disposed.current) {
        for (const track of media.getTracks()) track.stop();
        return;
      }
      stream.current = media;
      const rec = new MediaRecorder(media);
      chunks.current = [];
      keep.current = true;
      rec.ondataavailable = (event) => {
        if (event.data.size > 0) chunks.current.push(event.data);
      };
      rec.onstop = async () => {
        const wanted = keep.current;
        const blob = new Blob(chunks.current, { type: rec.mimeType || "audio/webm" });
        const durationMs = Date.now() - startedAt.current;
        const bytes = new Uint8Array(await blob.arrayBuffer());
        release();
        if (wanted && durationMs >= MIN_HOLD_MS && bytes.length > 0)
          latest.current.onFinished(bytes, blob.type, durationMs);
      };

      let analyser: AnalyserNode | null = null;
      if (typeof AudioContext !== "undefined") {
        try {
          const context = new AudioContext();
          analyser = context.createAnalyser();
          analyser.fftSize = 256;
          context.createMediaStreamSource(media).connect(analyser);
          audio.current = context;
        } catch {
          analyser = null;
        }
      }
      const samples = new Uint8Array(analyser?.fftSize ?? 0);

      recorder.current = rec;
      startedAt.current = Date.now();
      rec.start();
      setRecording(true);
      setElapsedMs(0);
      timer.current = setInterval(() => {
        setElapsedMs(Date.now() - startedAt.current);
        if (!analyser) return;
        analyser.getByteTimeDomainData(samples);
        let sum = 0;
        for (const s of samples) sum += ((s - 128) / 128) ** 2;
        const level = Math.min(1, Math.sqrt(sum / samples.length) * 4);
        setLevels((previous) => [...previous.slice(1), Math.max(0.08, level)]);
      }, 80);
      const early = stopWanted.current as { keep: boolean } | null;
      if (early) {
        keep.current = early.keep;
        rec.stop();
      }
    } catch (error) {
      release();
      const name = error instanceof DOMException || error instanceof Error ? error.name : "";
      latest.current.onFailed(DENIED.has(name) ? "denied" : "other");
    } finally {
      starting.current = false;
    }
  }, [release]);

  const stop = useCallback((keepIt: boolean) => {
    stopWanted.current = { keep: keepIt };
    keep.current = keepIt;
    const rec = recorder.current;
    if (rec && rec.state !== "inactive") rec.stop();
  }, []);

  useEffect(() => {
    disposed.current = false;
    return () => {
      disposed.current = true;
      stopWanted.current = { keep: false };
      keep.current = false;
      const rec = recorder.current;
      if (rec && rec.state !== "inactive") rec.stop();
      release();
    };
  }, [release]);

  return {
    recording,
    elapsedMs,
    levels,
    start: () => void start(),
    stop,
    starting: () => starting.current,
  };
}
