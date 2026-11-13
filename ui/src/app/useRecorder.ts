import { useCallback, useEffect, useRef, useState } from "react";

export type RecorderFailure = "denied" | "unsupported" | "other";

interface RecorderOptions {
  onFinished: (bytes: Uint8Array, mime: string, durationMs: number) => void;
  onFailed: (reason: RecorderFailure) => void;
}

const DENIED = new Set([
  "NotAllowedError",
  "SecurityError",
  "PermissionDeniedError",
  "NotFoundError",
]);

export function canRecord(): boolean {
  return typeof MediaRecorder !== "undefined" && Boolean(navigator.mediaDevices?.getUserMedia);
}

/** Records the microphone while a button is held. Nothing leaves the computer. */
export function useRecorder({ onFinished, onFailed }: RecorderOptions) {
  const [seconds, setSeconds] = useState<number | null>(null);
  const recorder = useRef<MediaRecorder | null>(null);
  const stream = useRef<MediaStream | null>(null);
  const chunks = useRef<Blob[]>([]);
  const startedAt = useRef(0);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const starting = useRef(false);
  const stopWanted = useRef(false);
  const latest = useRef({ onFinished, onFailed });
  latest.current = { onFinished, onFailed };

  const release = useCallback(() => {
    if (timer.current) clearInterval(timer.current);
    timer.current = null;
    for (const track of stream.current?.getTracks() ?? []) track.stop();
    stream.current = null;
    recorder.current = null;
    setSeconds(null);
  }, []);

  const start = useCallback(async () => {
    if (recorder.current || starting.current) return;
    if (!canRecord()) {
      latest.current.onFailed("unsupported");
      return;
    }
    starting.current = true;
    stopWanted.current = false;
    try {
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.current = media;
      const rec = new MediaRecorder(media);
      chunks.current = [];
      rec.ondataavailable = (event) => {
        if (event.data.size > 0) chunks.current.push(event.data);
      };
      rec.onstop = async () => {
        const blob = new Blob(chunks.current, { type: rec.mimeType || "audio/webm" });
        const durationMs = Date.now() - startedAt.current;
        const bytes = new Uint8Array(await blob.arrayBuffer());
        release();
        if (bytes.length > 0) latest.current.onFinished(bytes, blob.type, durationMs);
      };
      recorder.current = rec;
      startedAt.current = Date.now();
      rec.start();
      setSeconds(0);
      timer.current = setInterval(
        () => setSeconds(Math.floor((Date.now() - startedAt.current) / 1000)),
        500,
      );
      if (stopWanted.current) rec.stop();
    } catch (error) {
      release();
      const name = error instanceof DOMException || error instanceof Error ? error.name : "";
      latest.current.onFailed(DENIED.has(name) ? "denied" : "other");
    } finally {
      starting.current = false;
    }
  }, [release]);

  const stop = useCallback(() => {
    stopWanted.current = true;
    const rec = recorder.current;
    if (rec && rec.state !== "inactive") rec.stop();
  }, []);

  useEffect(() => () => release(), [release]);

  return { seconds, start: () => void start(), stop };
}
