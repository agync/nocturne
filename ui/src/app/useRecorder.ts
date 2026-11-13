import { useCallback, useEffect, useRef, useState } from "react";
import type { CopyId } from "../copy/t";
import type { RecordingState } from "../engine/client";

/** Why a recording did not happen. Only "insecure" and "unsupported" can be known in advance. */
export type RecorderFailure = "denied" | "unsupported" | "insecure" | "other";

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

export function recordingState(): RecordingState {
  // A page served over plain http from another computer is never given the microphone, whatever
  // the person allows.
  if (typeof window !== "undefined" && window.isSecureContext === false) return "not-secure";
  if (!navigator.mediaDevices?.getUserMedia) return "no-devices";
  if (typeof MediaRecorder === "undefined") return "no-recorder";
  return "available";
}

const MIME_TYPES = ["audio/webm;codecs=opus", "audio/webm", "audio/mp4", "audio/ogg;codecs=opus"];

/** The first sound format this browser can record: webm for Chrome and Firefox, mp4 for Safari. */
export function recordingOptions(): MediaRecorderOptions | undefined {
  if (typeof MediaRecorder.isTypeSupported !== "function") return undefined;
  const mimeType = MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type));
  return mimeType ? { mimeType } : undefined;
}

/** The reason recording cannot start, or null when it can. */
export function recordingBlock(): "insecure" | "unsupported" | null {
  const state = recordingState();
  if (state === "available") return null;
  return state === "not-secure" ? "insecure" : "unsupported";
}

/** The copy that explains a failure to the person. */
export function failureCopy(reason: RecorderFailure): CopyId {
  switch (reason) {
    case "insecure":
      return "capture.mic.insecure";
    case "unsupported":
      return "capture.mic.unsupported";
    case "denied":
      return "notice.micDenied.text";
    case "other":
      return "capture.mic.failed";
  }
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
  const told = useRef(false);
  const unavailable = recordingBlock();
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
    const block = recordingBlock();
    if (block) {
      // Said once; the button stays marked as unavailable, so more presses add nothing.
      if (!told.current) latest.current.onFailed(block);
      told.current = true;
      return;
    }
    starting.current = true;
    stopWanted.current = false;
    try {
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.current = media;
      const rec = new MediaRecorder(media, recordingOptions());
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

  return { seconds, start: () => void start(), stop, unavailable };
}
