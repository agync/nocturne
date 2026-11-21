import { hashHref } from "../../app/query";
import { BandLine } from "../../components/health/BandLine";
import charts from "../../components/health/Charts.module.css";
import { DayBars } from "../../components/health/DayBars";
import { nightsOf } from "../../components/health/derive";
import { NightsStrip } from "../../components/health/NightsStrip";
import {
  duration,
  kindName,
  nightsSentence,
  num,
  usualText,
  workoutsSentence,
} from "../../components/health/sentence";
import { type CopyId, t } from "../../copy/t";
import type { HealthSeries, ReadingKind, Recap, RecapHealth } from "../../engine/bindings";
import styles from "./Lookback.module.css";
import { Chips, useActiveReceipt } from "./Receipts";

const SLUG: Partial<Record<ReadingKind, string>> = {
  sleep: "sleep",
  steps: "steps",
  restingHeartRate: "resting-heart-rate",
};

const link = (kind: ReadingKind) => (day: string) =>
  hashHref(`/journal/health/${SLUG[kind] ?? ""}`, { day });

function Head({ label, say }: { label: string; say: string }) {
  return (
    <div className={styles.hfHead}>
      <span className={styles.hfName}>{label}</span>
      <span className={styles.hfSay}>{say}</span>
    </div>
  );
}

const average = (values: readonly number[]): number =>
  values.length === 0 ? 0 : values.reduce((a, b) => a + b, 0) / values.length;

const weekdayOf = (day: string): string =>
  new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(new Date(`${day}T12:00:00`));

const initialOf = (day: string): string =>
  new Intl.DateTimeFormat("en-GB", { weekday: "narrow" }).format(new Date(`${day}T12:00:00`));

function Sleep({
  series,
  health,
  long,
}: {
  series: HealthSeries;
  health: RecapHealth;
  long: boolean;
}) {
  const minutes = series.points.map((p) => p.value);
  const usual =
    series.usualBed && series.usualWake ? { bed: series.usualBed, wake: series.usualWake } : null;
  const nights = nightsOf(health.nights, series.journalDays);
  return (
    <div className={styles.hf}>
      <Head
        label={kindName("sleep")}
        say={t("journal.lookback.health.sleepSay", { average: duration(average(minutes)) })}
      />
      {long || nights.length === 0 ? (
        <p className={styles.meta}>{nightsSentence(minutes)}</p>
      ) : (
        <NightsStrip
          nights={nights}
          usual={usual}
          sentence={nightsSentence(minutes)}
          href={link("sleep")}
        />
      )}
    </div>
  );
}

type Kind = "week" | "month" | "year";

const STEPS_SENTENCE = {
  week: "journal.lookback.health.stepsSentence.week",
  month: "journal.lookback.health.stepsSentence.month",
  year: "journal.lookback.health.stepsSentence.year",
} as const satisfies Record<Kind, CopyId>;

const RESTING_SENTENCE = {
  week: "journal.lookback.health.restingSentence.week",
  month: "journal.lookback.health.restingSentence.month",
  year: "journal.lookback.health.restingSentence.year",
} as const satisfies Record<Kind, CopyId>;

const monthInitial = (month: string): string =>
  new Intl.DateTimeFormat("en-GB", { month: "narrow" }).format(new Date(`${month}-15T12:00:00`));

/** A year of steps as twelve bars, each a month's average day: 365 bars would be a smear. */
function MonthBars({ series, sentence }: { series: HealthSeries; sentence: string }) {
  const year = series.from.slice(0, 4);
  const months = Array.from({ length: 12 }, (_, i) => `${year}-${String(i + 1).padStart(2, "0")}`);
  const means = months.map((m) => {
    const days = series.points.filter((p) => p.day.startsWith(m));
    return days.length === 0 ? null : average(days.map((p) => p.value));
  });
  const top = Math.max(1, ...means.map((v) => v ?? 0));
  const width = 360;
  const height = 80;
  const bottom = height - 18;
  const step = width / 12;
  return (
    <div className={charts.frame}>
      <svg
        className={charts.svg}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={sentence}
        data-chart="bars"
      >
        {months.map((m, i) => {
          const value = means[i];
          const x = i * step + step / 2;
          return (
            <g key={m}>
              {value === null || value === undefined ? null : (
                <rect
                  className={charts.bar}
                  data-tone="soft"
                  x={x - 10}
                  y={bottom - (value / top) * (bottom - 14)}
                  width={20}
                  height={(value / top) * (bottom - 14)}
                  rx={3}
                />
              )}
              <text className={charts.axis} x={x} y={height - 2} textAnchor="middle">
                {monthInitial(m)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function Steps({ series, kind }: { series: HealthSeries; kind: Kind }) {
  const most = [...series.points].sort((a, b) => b.value - a.value)[0];
  const mean = Math.round(average(series.points.map((p) => p.value)) / 100) * 100;
  const sentence = t(STEPS_SENTENCE[kind], {
    average: num(mean),
    day: most ? weekdayOf(most.day) : "",
  });
  return (
    <div className={styles.hf}>
      <Head
        label={kindName("steps")}
        say={t("journal.lookback.health.stepsSay", { average: num(mean) })}
      />
      {kind === "year" ? (
        <MonthBars series={series} sentence={sentence} />
      ) : (
        <DayBars
          points={series.points}
          from={series.from}
          to={series.to}
          at={kind === "week" ? most?.day : undefined}
          journalDays={series.journalDays}
          label={kind === "week" ? initialOf : undefined}
          sentence={sentence}
          width={360}
          height={80}
          href={link("steps")}
        />
      )}
    </div>
  );
}

function Resting({ series, kind }: { series: HealthSeries; kind: Kind }) {
  const values = series.points.map((p) => p.value);
  const sentence = `${t(RESTING_SENTENCE[kind], {
    low: num(Math.min(...values)),
    high: num(Math.max(...values)),
  })}${series.usual ? ` ${usualText(series.usual)}.` : ""}`;
  return (
    <div className={styles.hf}>
      <Head
        label={kindName("restingHeartRate")}
        say={series.usual ? usualText(series.usual).toLowerCase() : ""}
      />
      <BandLine
        points={series.points}
        from={series.from}
        to={series.to}
        band={series.usual}
        min={40}
        max={Math.max(80, ...values.map((v) => v + 8))}
        journalDays={series.journalDays}
        sentence={sentence}
        width={360}
        height={80}
        href={link("restingHeartRate")}
      />
    </div>
  );
}

/** "From Health" in a recap: the nights, steps, resting heart rate and workouts, with receipts. */
export function HealthSection({ recap }: { recap: Recap }) {
  const receipt = useActiveReceipt();
  const health = recap.health;
  if (!health) return null;
  const kind = recap.period.kind as Kind;
  const long = kind === "year";
  const { sleep, steps, resting, workouts } = health;
  return (
    <section className={styles.section} aria-labelledby="sec-health">
      <h2 id="sec-health" className={styles.label}>
        {t("health.fromHealth")}
      </h2>
      {sleep ? <Sleep series={sleep} health={health} long={long} /> : null}
      {steps || resting ? (
        <div className={styles.hfPair}>
          {steps ? <Steps series={steps} kind={kind} /> : null}
          {resting ? <Resting series={resting} kind={kind} /> : null}
        </div>
      ) : null}
      {workouts.length > 0 ? (
        <div className={styles.hf}>
          <p className={styles.meta}>{workoutsSentence(workouts.map((w) => w.label))}</p>
          <Chips
            receipts={workouts}
            receipt={receipt}
            withDay={kind === "week" ? "weekday" : "date"}
            most={2}
          />
        </div>
      ) : null}
    </section>
  );
}
