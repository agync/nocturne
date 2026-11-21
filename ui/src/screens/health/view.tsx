import type { ReactNode } from "react";
import {
  BandLine,
  between,
  clock,
  DayBars,
  DayRange,
  dayIndex,
  dayLabel,
  duration,
  HourBars,
  hoursSentence,
  hourWord,
  kindName,
  type ListRow,
  lineSentence,
  NightsStrip,
  nightsOf,
  nightsSentence,
  num,
  pressureSentence,
  pressureText,
  pressureWhen,
  ReadingsStrip,
  rangeSentence,
  stepsSentence,
  workoutsSentence,
} from "../../components/health";
import { formatDay, formatDayMonth } from "../../copy/format";
import { t } from "../../copy/t";
import type { Band, HealthSeries, ReadingKind, ReadingRow } from "../../engine/bindings";

/** What a kind's page draws: the chart, its sentence, and the same numbers as a list. */
export interface KindView {
  sentence: string;
  chart: ReactNode;
  columns: readonly string[];
  rows: readonly ListRow[];
  /** The kind has a usual and there is not enough to show it yet. */
  waiting: boolean;
}

export interface ViewInput {
  kind: ReadingKind;
  /** 90 days or the year, drawn as a line rather than a strip of nights. */
  long: boolean;
  from: string;
  to: string;
  today: string;
  /** The readings of the range, newest first, with those removed in Health left out. */
  readings: readonly ReadingRow[];
  series: HealthSeries | null;
  /** The workouts of the day the heart rate chart shows. */
  workouts: readonly ReadingRow[];
}

const columns = () =>
  [t("health.list.day"), t("health.list.value"), t("health.list.time")] as const;

const labelsOf = (from: string, to: string) => [
  { day: from, text: formatDayMonth(from) },
  { day: to, text: formatDayMonth(to) },
];

const hourOf = (instant: string) => new Date(instant).getHours();

/** The hours a workout covers, within its own day. */
function hoursOfWorkout(row: ReadingRow): number[] {
  if (row.value.type !== "workout") return [];
  const first = hourOf(row.start);
  const last = Math.min(
    23,
    hourOf(new Date(Date.parse(row.start) + row.value.lengthS * 1000).toISOString()),
  );
  const hours: number[] = [];
  for (let h = first; h <= Math.max(first, last); h += 1) hours.push(h);
  return hours;
}

function sleepView(input: ViewInput): KindView | null {
  const { readings, series, from, to, today, long } = input;
  const journalDays = series?.journalDays ?? [];
  if (!long) {
    const nights = nightsOf(readings, journalDays);
    const last = nights[nights.length - 1];
    if (!last) return null;
    const usual =
      series?.usualBed && series.usualWake
        ? { bed: series.usualBed, wake: series.usualWake }
        : null;
    const sentence = nightsSentence(nights.map((n) => n.asleepMin));
    return {
      sentence,
      waiting: usual === null,
      chart: <NightsStrip nights={nights} usual={usual} sentence={sentence} current={last.day} />,
      columns: columns(),
      rows: [...nights].reverse().map((n) => ({
        key: n.day,
        cells: [dayLabel(n.day), duration(n.asleepMin), between(clock(n.from), clock(n.to))],
      })),
    };
  }
  const points = series?.points ?? [];
  const last = points[points.length - 1];
  if (!last) return null;
  const band = series?.usual ?? null;
  const top = Math.max(...points.map((p) => p.value), band?.high ?? 0);
  const sentence = lineSentence({
    label: kindName("sleep"),
    value: duration(last.value),
    day: last.day,
    today,
    band,
    format: duration,
  });
  return {
    sentence,
    waiting: band === null,
    chart: (
      <BandLine
        points={points}
        from={from}
        to={to}
        band={band}
        min={0}
        max={Math.ceil(top / 60) * 60}
        journalDays={journalDays}
        format={duration}
        sentence={sentence}
        labels={labelsOf(from, to)}
      />
    ),
    columns: [t("health.list.day"), t("health.list.value")],
    rows: [...points].reverse().map((p) => ({
      key: p.day,
      cells: [formatDay(p.day), duration(p.value)],
    })),
  };
}

function heartView(input: ViewInput): KindView | null {
  const latest = input.readings[0];
  if (latest?.value.type !== "heartRate") return null;
  const hours = latest.value.hours;
  if (hours.length === 0) return null;
  const low = Math.min(...hours.map((h) => h.low));
  const high = Math.max(...hours.map((h) => h.high));
  const middle = Math.round(hours.reduce((sum, h) => sum + h.middle, 0) / hours.length);
  const peak = hours.reduce((a, b) => (b.high > a.high ? b : a));
  const peakHour = hourOf(peak.start);
  const covering = input.workouts.find((w) => hoursOfWorkout(w).includes(peakHour));
  const workoutHours = input.workouts.flatMap(hoursOfWorkout);
  const range = rangeSentence({ low, high, middle, day: latest.day, today: input.today });
  const byHour = hoursSentence({
    day: latest.day,
    today: input.today,
    highest: peakHour,
    workout: covering?.value.type === "workout" ? covering.value.kind.toLowerCase() : null,
  });
  return {
    sentence: `${range} ${byHour}`,
    waiting: false,
    chart: (
      <>
        <DayRange low={low} high={high} middle={middle} sentence={range} />
        <HourBars hours={hours} workoutHours={workoutHours} sentence={byHour} />
      </>
    ),
    columns: [t("health.list.time"), t("health.list.value")],
    rows: [...hours]
      .sort((a, b) => a.start.localeCompare(b.start))
      .map((h) => ({
        key: h.start,
        cells: [hourWord(hourOf(h.start)), between(num(h.low), num(h.high))],
      })),
  };
}

function lineView(
  input: ViewInput,
  floor: (values: number[], band: Band | null) => [number, number],
) {
  const { kind, series, from, to, today } = input;
  const points = series?.points ?? [];
  const last = points[points.length - 1];
  if (!last) return null;
  const weight = input.readings.find((r) => r.value.type === "weight")?.value;
  const unit = weight?.type === "weight" ? weight.unit : "";
  const format = (v: number) => (kind === "weight" ? `${num(v)} ${unit}`.trim() : num(v));
  const band = series?.usual ?? null;
  const [min, max] = floor(
    points.map((p) => p.value),
    band,
  );
  const sentence = lineSentence({
    label: kindName(kind),
    value: format(last.value),
    day: last.day,
    today,
    band,
    format,
  });
  return {
    sentence,
    waiting: band === null,
    chart: (
      <BandLine
        points={points}
        from={from}
        to={to}
        band={band}
        min={min}
        max={max}
        journalDays={series?.journalDays ?? []}
        format={format}
        sentence={sentence}
        labels={labelsOf(from, to)}
      />
    ),
    columns: [t("health.list.day"), t("health.list.value")],
    rows: [...points]
      .reverse()
      .map((p) => ({ key: p.day, cells: [formatDay(p.day), format(p.value)] })),
  } satisfies KindView;
}

function pressureView(input: ViewInput): KindView | null {
  const { readings, from, to, today, series } = input;
  const points = readings.flatMap((r) =>
    r.value.type === "bloodPressure"
      ? [{ day: r.day, at: r.start, top: r.value.top, bottom: r.value.bottom }]
      : [],
  );
  const latest = points[0];
  if (!latest) return null;
  const bandTop = series?.pressureUsual?.top ?? null;
  const bandBottom = series?.pressureUsual?.bottom ?? null;
  const sentence = pressureSentence({
    top: latest.top,
    bottom: latest.bottom,
    when: pressureWhen(latest.at, today, latest.day),
    bandTop: bandTop && bandBottom ? bandTop : null,
    bandBottom: bandTop && bandBottom ? bandBottom : null,
  });
  return {
    sentence,
    waiting: !bandTop || !bandBottom,
    chart: (
      <ReadingsStrip
        readings={points}
        from={from}
        to={to}
        bandTop={bandTop}
        bandBottom={bandBottom}
        journalDays={series?.journalDays ?? []}
        ring={latest.day}
        labels={labelsOf(from, to)}
        sentence={sentence}
      />
    ),
    columns: columns(),
    rows: points.map((p) => ({
      key: `${p.at}`,
      cells: [formatDay(p.day), pressureText(p.top, p.bottom), clock(p.at)],
    })),
  };
}

function stepsView(input: ViewInput): KindView | null {
  const { series, from, to, today } = input;
  const points = series?.points ?? [];
  if (points.length === 0) return null;
  const focus = points.find((p) => p.day === today) ?? points[points.length - 1];
  if (!focus) return null;
  const average = points.reduce((sum, p) => sum + p.value, 0) / points.length;
  const sentence = stepsSentence({
    steps: focus.value,
    day: focus.day,
    today,
    average,
    days: dayIndex(from, to) + 1,
  });
  return {
    sentence,
    waiting: false,
    chart: (
      <DayBars
        points={points}
        from={from}
        to={to}
        at={focus.day}
        journalDays={series?.journalDays ?? []}
        sentence={sentence}
      />
    ),
    columns: [t("health.list.day"), t("health.list.value")],
    rows: [...points]
      .reverse()
      .map((p) => ({ key: p.day, cells: [formatDay(p.day), num(p.value)] })),
  };
}

/** Resting heart rate starts at 40 and weight a tenth under its lowest reading, neither cropped to a change. */
const restingScale = (values: number[], band: Band | null): [number, number] => [
  40,
  Math.max(...values, band?.high ?? 0) + 10,
];

/**
 * A line from zero would flatten a weight to nothing, so the floor is a tenth under the lowest
 * reading or usual: wide enough that a change shows at its true size, never tightened to the data.
 */
const weightScale = (values: number[], band: Band | null): [number, number] => {
  const lowest = Math.min(...values, band?.low ?? Number.POSITIVE_INFINITY);
  const highest = Math.max(...values, band?.high ?? 0);
  return [Math.floor(lowest * 0.9), Math.ceil(highest * 1.1)];
};

/** What the page draws for a kind, or nothing when there are no readings in the range. */
export function buildView(input: ViewInput): KindView | null {
  switch (input.kind) {
    case "sleep":
      return sleepView(input);
    case "heartRate":
      return heartView(input);
    case "restingHeartRate":
      return lineView(input, restingScale);
    case "weight":
      return lineView(input, weightScale);
    case "bloodPressure":
      return pressureView(input);
    case "steps":
      return stepsView(input);
    case "workout":
      return null;
  }
}

/** The workouts sentence: counted by kind, as the recap says it. */
export function workoutsLine(readings: readonly ReadingRow[]): string {
  return workoutsSentence(
    readings.flatMap((r) => (r.value.type === "workout" ? [r.value.kind] : [])),
  );
}
