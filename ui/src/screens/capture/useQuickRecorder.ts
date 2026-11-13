import { useCallback, useEffect, useRef, useState } from "react";
import { type RecorderFailure, recordingBlock } from "../../app/useRecorder";
import { startWavCapture, type WavCapture } from "../../app/wavCapture";

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
  const capture = useRef<WavCapture | null>(null);
  const stream = useRef<MediaStream | null>(null);
  const startedAt = useRef(0);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const starting = useRef(false);
  const stopWanted = useRef<null | { keep: boolean }>(null);
  const disposed = useRef(false);
  const told = useRef(false);
  const unavailable = recordingBlock();
  const latest = useRef({ onFinished, onFailed });
  latest.current = { onFinished, onFailed };

  const release = useCallback(() => {
    if (timer.current) clearInterval(timer.current);
    timer.current = null;
    capture.current?.discard();
    capture.current = null;
    for (const track of stream.current?.getTracks() ?? []) track.stop();
    stream.current = null;
    setRecording(false);
    setElapsedMs(0);
    setLevels(quiet());
  }, []);

  const finish = useCallback(
    (keepIt: boolean) => {
      const open = capture.current;
      if (!open) return;
      const durationMs = Date.now() - startedAt.current;
      const bytes = open.finish();
      capture.current = null;
      release();
      if (keepIt && durationMs >= MIN_HOLD_MS && bytes)
        latest.current.onFinished(bytes, "audio/wav", durationMs);
    },
    [release],
  );

  const start = useCallback(async () => {
    if (capture.current || starting.current) return;
    const block = recordingBlock();
    if (block) {
      if (!told.current) latest.current.onFailed(block);
      told.current = true;
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
      const open = startWavCapture(media, () => finish(true));
      capture.current = open;
      const analyser = open.analyser;
      const samples = new Uint8Array(analyser?.fftSize ?? 0);
      startedAt.current = Date.now();
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
      if (early) finish(early.keep);
    } catch (error) {
      release();
      const name = error instanceof DOMException || error instanceof Error ? error.name : "";
      latest.current.onFailed(DENIED.has(name) ? "denied" : "other");
    } finally {
      starting.current = false;
    }
  }, [release, finish]);

  const stop = useCallback(
    (keepIt: boolean) => {
      stopWanted.current = { keep: keepIt };
      finish(keepIt);
    },
    [finish],
  );

  useEffect(() => {
    disposed.current = false;
    return () => {
      disposed.current = true;
      stopWanted.current = { keep: false };
      release();
    };
  }, [release]);

  return {
    recording,
    elapsedMs,
    levels,
    start: () => void start(),
    stop,
    unavailable,
    starting: () => starting.current,
  };
}
