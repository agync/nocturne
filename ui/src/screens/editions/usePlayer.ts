import {
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { SpokenSection } from "../../engine/bindings";
import { SPEEDS } from "../settings/voices";

/** How far the arrow keys move. */
export const SKIP_SECONDS = 10;
/** Pressing Previous this far into a section goes back to its start; sooner goes to the one before. */
const RESTART_AFTER_SECONDS = 3;

export interface Player {
  playing: boolean;
  /** Seconds. */
  time: number;
  /** Seconds. */
  duration: number;
  rate: number;
  /** The section the position is in. */
  index: number;
  toggle(): void;
  seek(seconds: number): void;
  skipBy(seconds: number): void;
  jump(index: number): void;
  next(): void;
  previous(): void;
  faster(): void;
  slower(): void;
  /** Handles a key press for the player. Returns true when it did something. */
  onKey(event: KeyboardEvent | ReactKeyboardEvent): boolean;
}

interface Options {
  audio: React.RefObject<HTMLAudioElement | null>;
  sections: SpokenSection[];
  /** What the engine says the audio lasts, used until the file reports its own length. */
  durationMs: number;
  /** The speed to start at. */
  rate: number;
  /** Called when the person changes the speed. */
  onRate(rate: number): void;
}

/** The section a position falls in: the last one that has started. */
export function sectionAt(sections: SpokenSection[], seconds: number): number {
  let found = 0;
  sections.forEach((s, i) => {
    if (s.startMs / 1000 <= seconds + 0.001) found = i;
  });
  return found;
}

/** The speed one step slower or faster in the list, staying inside it. */
export function stepSpeed(rate: number, direction: 1 | -1): number {
  const at = SPEEDS.reduce(
    (best, speed, i) => (Math.abs(speed - rate) < Math.abs((SPEEDS[best] ?? 0) - rate) ? i : best),
    0,
  );
  const next = Math.min(SPEEDS.length - 1, Math.max(0, at + direction));
  return SPEEDS[next] ?? rate;
}

/** Plays the spoken page through an `<audio>` element and keeps what the screen shows in step. */
export function usePlayer({ audio, sections, durationMs, rate: initial, onRate }: Options): Player {
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [length, setLength] = useState(durationMs / 1000);
  const [rate, setRate] = useState(initial);
  const rateRef = useRef(initial);
  const latest = useRef({ sections, onRate });
  latest.current = { sections, onRate };

  useEffect(() => setLength(durationMs / 1000), [durationMs]);

  useEffect(() => {
    const el = audio.current;
    if (!el) return;
    el.playbackRate = rate;
    const onTime = () => setTime(el.currentTime);
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onLength = () => {
      if (Number.isFinite(el.duration) && el.duration > 0) setLength(el.duration);
    };
    el.addEventListener("timeupdate", onTime);
    el.addEventListener("seeked", onTime);
    el.addEventListener("play", onPlay);
    el.addEventListener("pause", onPause);
    el.addEventListener("ended", onPause);
    el.addEventListener("loadedmetadata", onLength);
    el.addEventListener("durationchange", onLength);
    return () => {
      el.removeEventListener("timeupdate", onTime);
      el.removeEventListener("seeked", onTime);
      el.removeEventListener("play", onPlay);
      el.removeEventListener("pause", onPause);
      el.removeEventListener("ended", onPause);
      el.removeEventListener("loadedmetadata", onLength);
      el.removeEventListener("durationchange", onLength);
    };
    // The speed is applied below, so a change does not re-attach the listeners.
  }, [audio]);

  useEffect(() => {
    const el = audio.current;
    if (el) el.playbackRate = rate;
  }, [audio, rate]);

  const seek = useCallback(
    (seconds: number) => {
      const el = audio.current;
      const to = Math.min(Math.max(0, seconds), length);
      if (el) el.currentTime = to;
      setTime(to);
    },
    [audio, length],
  );

  const toggle = useCallback(() => {
    const el = audio.current;
    if (!el) return;
    if (el.paused) void Promise.resolve(el.play()).catch(() => setPlaying(false));
    else el.pause();
  }, [audio]);

  const index = sectionAt(sections, time);

  const jump = useCallback(
    (to: number) => {
      const target =
        latest.current.sections[Math.min(Math.max(0, to), latest.current.sections.length - 1)];
      if (target) seek(target.startMs / 1000);
    },
    [seek],
  );

  const next = useCallback(() => {
    if (index < sections.length - 1) jump(index + 1);
  }, [index, sections.length, jump]);

  const previous = useCallback(() => {
    const start = (sections[index]?.startMs ?? 0) / 1000;
    if (time - start > RESTART_AFTER_SECONDS || index === 0) jump(index);
    else jump(index - 1);
  }, [index, sections, time, jump]);

  const changeRate = useCallback((direction: 1 | -1) => {
    const next = stepSpeed(rateRef.current, direction);
    if (next === rateRef.current) return;
    rateRef.current = next;
    setRate(next);
    latest.current.onRate(next);
  }, []);

  const onKey = useCallback(
    (event: KeyboardEvent | ReactKeyboardEvent): boolean => {
      if (event.ctrlKey || event.metaKey || event.altKey) return false;
      switch (event.key) {
        case " ":
          toggle();
          break;
        case "ArrowLeft":
          if (event.shiftKey) previous();
          else seek(time - SKIP_SECONDS);
          break;
        case "ArrowRight":
          if (event.shiftKey) next();
          else seek(time + SKIP_SECONDS);
          break;
        case "[":
          changeRate(-1);
          break;
        case "]":
          changeRate(1);
          break;
        default:
          return false;
      }
      event.preventDefault();
      return true;
    },
    [toggle, previous, next, seek, time, changeRate],
  );

  return {
    playing,
    time,
    duration: length,
    rate,
    index,
    toggle,
    seek,
    skipBy: (seconds) => seek(time + seconds),
    jump,
    next,
    previous,
    faster: () => changeRate(1),
    slower: () => changeRate(-1),
    onKey,
  };
}
