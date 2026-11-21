// The sentence of every health chart: what a screen reader hears as the chart's name, and what
// shows under the chart. Plain numbers and the person's own units; never a judgement.

import { formatLongDay, formatTime12, formatWeekday } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { Band, NightLine, ReadingKind, Versus } from "../../engine/bindings";

const NUMBER = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 1 });

/** "8,412": the person's grouping, no decimals unless the value has them. */
export const num = (value: number): string => NUMBER.format(value);

/** "6 h 40 min", "5 h 05 min" or "40 min". */
export function duration(minutes: number): string {
  const total = Math.max(0, Math.round(minutes));
  const hours = Math.floor(total / 60);
  const rest = total % 60;
  if (hours === 0) return t("health.duration.m", { minutes: rest });
  return t("health.duration.hm", { hours, minutes: String(rest).padStart(2, "0") });
}

/** "11:50 pm", from an instant. */
export const clock = (instant: string): string => formatTime12(instant);

/** "Sat 7": the weekday and the day of the month, as the nights strip and its list label a night. */
export function dayLabel(day: string): string {
  return `${formatWeekday(day)} ${Number(day.slice(8, 10))}`;
}

const KIND_NAMES = {
  sleep: "health.kind.sleep",
  heartRate: "health.kind.heartRate",
  restingHeartRate: "health.kind.restingHeartRate",
  bloodPressure: "health.kind.bloodPressure",
  steps: "health.kind.steps",
  workout: "health.kind.workout",
  weight: "health.kind.weight",
} as const satisfies Record<ReadingKind, CopyId>;

/** The kind's name: "Resting heart rate". */
export const kindName = (kind: ReadingKind): string => t(KIND_NAMES[kind]);

const VERSUS = {
  more: "health.versus.more",
  less: "health.versus.less",
  higher: "health.versus.higher",
  lower: "health.versus.lower",
} as const satisfies Record<Versus, CopyId>;

/** "more than your usual", for a value outside the band; nothing when inside it. */
export const versusText = (versus: Versus | null): string => (versus ? t(VERSUS[versus]) : "");

/** "6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once." */
export function nightSentence(night: NightLine): string {
  return t("health.night.sentence", {
    asleep: duration(night.asleepMin),
    from: clock(night.from),
    to: clock(night.to),
    count: night.awakeCount,
  });
}

/** The page's line, under the title: "6 h 40 min asleep, 11:50 pm to 7:05 am". */
export function nightLine(night: NightLine): string {
  return t("health.night.line", {
    asleep: duration(night.asleepMin),
    from: clock(night.from),
    to: clock(night.to),
  });
}

/** "7 nights. Asleep 6 h 50 min on an average night, between 5 h 10 min and 8 h 05 min." */
export function nightsSentence(minutes: readonly number[]): string {
  if (minutes.length === 0) return "";
  const sum = minutes.reduce((a, b) => a + b, 0);
  return t("health.nights.sentence", {
    count: minutes.length,
    average: duration(sum / minutes.length),
    low: duration(Math.min(...minutes)),
    high: duration(Math.max(...minutes)),
  });
}

/** "your usual is 55 to 61": only when there is a band. */
export function usualText(
  band: Band | null,
  format: (v: number) => string = (v) => num(Math.round(v)),
): string {
  return band ? t("health.usual", { low: format(band.low), high: format(band.high) }) : "";
}

/** "today" or "on Friday 13 November". */
export function whenText(day: string, today: string): string {
  return day === today ? t("health.when.today") : t("health.when.day", { day: formatLongDay(day) });
}

/** "Resting heart rate, 58 today. Your usual is 55 to 61." */
export function lineSentence(opts: {
  label: string;
  value: string;
  day: string;
  today: string;
  band: Band | null;
  format?: (v: number) => string;
}): string {
  const head = t("health.line.sentence", {
    label: opts.label,
    value: opts.value,
    when: whenText(opts.day, opts.today),
  });
  const usual = usualText(opts.band, opts.format);
  return usual ? `${head} ${usual}.` : head;
}

/** "Heart rate today, 52 to 148, mostly around 74." */
export function rangeSentence(opts: {
  low: number;
  high: number;
  middle: number;
  day: string;
  today: string;
}): string {
  return t("health.range.sentence", {
    when: whenText(opts.day, opts.today),
    low: num(opts.low),
    high: num(opts.high),
    middle: num(opts.middle),
  });
}

/** "a walk", "an hour" style: the article follows the first letter. */
const withArticle = (word: string): string => `${/^[aeiou]/i.test(word) ? "an" : "a"} ${word}`;

/** "Heart rate by hour today. Highest between 10 and 11 am, during a walk." */
export function hoursSentence(opts: {
  day: string;
  today: string;
  /** The hour of the highest reading, 0 to 23, and the workout that covers it, if any. */
  highest: number | null;
  workout: string | null;
}): string {
  const head = t("health.hours.sentence", { when: whenText(opts.day, opts.today) });
  if (opts.highest === null) return head;
  const { from, to } = hourSpan(opts.highest);
  const tail = opts.workout
    ? t("health.hours.during", { from, to, kind: withArticle(opts.workout) })
    : t("health.hours.highest", { from, to });
  return `${head} ${tail}`;
}

/** "10" and "11 am" for the hour from 10 to 11 in the morning; both ends spelt out when they cross noon or midnight. */
export function hourSpan(hour: number): { from: string; to: string } {
  const start = ((hour % 24) + 24) % 24;
  const end = (start + 1) % 24;
  const bare = start % 12 === 0 ? 12 : start % 12;
  const crosses = start === 11 || start === 23;
  return {
    from: crosses ? hourWord(start) : String(bare),
    to: hourWord(end),
  };
}

/** "10 am", "12 pm", "12 am" from an hour of the day. */
export function hourWord(hour: number): string {
  const h = ((hour % 24) + 24) % 24;
  const twelve = h % 12 === 0 ? 12 : h % 12;
  return t(h < 12 ? "health.hour.am" : "health.hour.pm", { hour: twelve });
}

/** "128 over 82". */
export const pressureText = (top: number, bottom: number): string =>
  t("health.pressure", { top: num(top), bottom: num(bottom) });

/** "this morning", "this afternoon", "this evening", or "on Wednesday 18 November". */
export function pressureWhen(at: string, today: string, day: string): string {
  if (day !== today) return t("health.when.day", { day: formatLongDay(day) });
  const hour = new Date(at).getHours();
  if (hour < 12) return t("health.when.morning");
  if (hour < 18) return t("health.when.afternoon");
  return t("health.when.evening");
}

/** "128 over 82 this morning. Your usual is 118 to 126 over 76 to 82." */
export function pressureSentence(opts: {
  top: number;
  bottom: number;
  when: string;
  bandTop: Band | null;
  bandBottom: Band | null;
}): string {
  const head = t("health.pressure.sentence", {
    reading: pressureText(opts.top, opts.bottom),
    when: opts.when,
  });
  if (!opts.bandTop || !opts.bandBottom) return head;
  return `${head} ${t("health.pressure.usual", {
    topLow: num(Math.round(opts.bandTop.low)),
    topHigh: num(Math.round(opts.bandTop.high)),
    bottomLow: num(Math.round(opts.bandBottom.low)),
    bottomHigh: num(Math.round(opts.bandBottom.high)),
  })}`;
}

/** "this fortnight" or "in the last 30 days". */
export function stepsPeriod(days: number): string {
  if (days <= 14) return t("health.steps.period.fortnight");
  return t("health.steps.period.days", { count: days });
}

/** "58 beats a minute": the unit spelt out, for the first time a number is read. */
export const beatsText = (value: number): string => t("health.beats", { value: num(value) });

/** "8,412 steps" or "1 step". */
export const stepsText = (count: number): string => t("health.steps.count", { count });

/** "8,412 steps today. 6,900 on an average day this fortnight." */
export function stepsSentence(opts: {
  steps: number;
  day: string;
  today: string;
  average: number;
  days: number;
}): string {
  return t("health.steps.sentence", {
    steps: stepsText(opts.steps),
    when: whenText(opts.day, opts.today),
    average: num(Math.round(opts.average / 100) * 100),
    period: stepsPeriod(opts.days),
  });
}

/** "Waiting" line shown instead of the band until two weeks of readings. */
export const usualWaiting = (): string => t("health.usual.waiting");

/** "52 to 148", "11:50 pm to 7:05 am". */
export const between = (low: string, high: string): string => t("health.between", { low, high });

/** "Walk · 42 min · 3.1 km" or "Swim · 35 min". */
export function workoutText(kind: string, lengthS: number, distanceM: number | null): string {
  const length = duration(lengthS / 60);
  if (distanceM === null) return t("health.workout", { kind, length });
  const km = num(Math.round(distanceM / 100) / 10);
  return t("health.workout.distance", { kind, length, distance: t("health.distance", { km }) });
}

const WORKOUT_COUNTS = {
  walk: "health.workouts.walk",
  run: "health.workouts.run",
  swim: "health.workouts.swim",
  cycle: "health.workouts.cycle",
  hike: "health.workouts.hike",
  other: "health.workouts.other",
} as const satisfies Record<string, CopyId>;

/** "4 walks, 1 swim", counted by kind in the order each first appears; kinds without a word of their own count as workouts. */
export function workoutsSentence(kinds: readonly string[]): string {
  const counts = new Map<keyof typeof WORKOUT_COUNTS, number>();
  for (const kind of kinds) {
    const lower = kind.toLowerCase();
    const key =
      lower in WORKOUT_COUNTS && lower !== "other"
        ? (lower as keyof typeof WORKOUT_COUNTS)
        : "other";
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  if (counts.size === 0) return "";
  const list = [...counts].map(([key, count]) => t(WORKOUT_COUNTS[key], { count }));
  return t("health.workouts.sentence", { counts: list.join(", ") });
}
