import { hourWord } from "./sentence";

/** The hours marked under a night: 10 pm, midnight, 2, 4, 6, 8 and 10 am, as hours after 8 pm. */
export const NIGHT_MARKS: ReadonlyArray<{ at: number; hour: number }> = [
  { at: 2, hour: 22 },
  { at: 4, hour: 24 },
  { at: 6, hour: 26 },
  { at: 8, hour: 28 },
  { at: 10, hour: 30 },
  { at: 12, hour: 32 },
  { at: 14, hour: 34 },
];

export const markLabel = (hour: number): string => hourWord(hour);

/** Every other mark when the track is small. */
export function marks(compact: boolean) {
  return compact ? NIGHT_MARKS.filter((_, i) => i % 2 === 0) : NIGHT_MARKS;
}
