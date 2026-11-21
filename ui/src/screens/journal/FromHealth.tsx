import { BandLine } from "../../components/health/BandLine";
import { DayBars } from "../../components/health/DayBars";
import { DayRange } from "../../components/health/DayRange";
import { NightBar } from "../../components/health/NightBar";
import {
  duration,
  kindName,
  lineSentence,
  nightSentence,
  num,
  pressureText,
  rangeSentence,
  stepsSentence,
  usualText,
  usualWaiting,
  versusText,
} from "../../components/health/sentence";
import { addDays } from "../../copy/format";
import { t } from "../../copy/t";
import type { DayHealth, HealthSeries, ReadingKind } from "../../engine/bindings";
import styles from "./FromHealth.module.css";
import { clock12 } from "./time";

const SLUG: Record<ReadingKind, string> = {
  sleep: "sleep",
  heartRate: "heart-rate",
  restingHeartRate: "resting-heart-rate",
  bloodPressure: "blood-pressure",
  steps: "steps",
  workout: "workouts",
  weight: "weight",
};

function More({ kind }: { kind: ReadingKind }) {
  return (
    <a className={styles.more} href={`#/journal/health/${SLUG[kind]}`}>
      {t("health.seeMore")}
      <span className="sr-only"> {kindName(kind)}</span>
    </a>
  );
}

function Head({ label, say }: { label: string; say: string }) {
  return (
    <div className={styles.head}>
      <span className={styles.name}>{label}</span>
      <span className={styles.say}>{say}</span>
    </div>
  );
}

const lastOf = (series: HealthSeries) => series.points[series.points.length - 1];

/** The days a series spans, for the labels under its axis. */
function axisLabels(series: HealthSeries, todayLabel: string) {
  const middle = addDays(series.from, Math.floor(daysBetween(series.from, series.to) / 2));
  return [
    { day: series.from, text: shortDay(series.from) },
    { day: middle, text: shortDay(middle) },
    { day: series.to, text: todayLabel },
  ];
}

const daysBetween = (a: string, b: string) =>
  Math.round((Date.parse(`${b}T12:00:00Z`) - Date.parse(`${a}T12:00:00Z`)) / 86_400_000);

const shortDay = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" }).format(
    new Date(`${day}T12:00:00`),
  );

/** "From Health": the day's night, resting heart rate, steps, heart rate range and blood pressure. */
export function FromHealth({
  health,
  day,
  isToday,
}: {
  health: DayHealth;
  day: string;
  isToday: boolean;
}) {
  const { night, resting, steps, heart, pressure } = health;
  const nothing = !night && !resting && !steps && !heart && pressure.length === 0;
  if (nothing) return null;
  const today = isToday ? day : "";
  const restingNow = resting ? lastOf(resting) : undefined;
  const stepsNow = steps ? lastOf(steps) : undefined;
  const stepAverage = steps
    ? steps.points.reduce((sum, p) => sum + p.value, 0) / Math.max(1, steps.points.length)
    : 0;
  return (
    <section className={styles.block} aria-labelledby="side-health">
      <h2 className={styles.label} id="side-health">
        {t("health.fromHealth")}
      </h2>

      {night ? (
        <div className={styles.item}>
          <Head
            label={isToday ? t("page.lastNight.label") : kindName("sleep")}
            say={`${t("journal.health.asleep", { asleep: duration(night.asleepMin) })}${
              versusText(night.versus) ? ` · ${versusText(night.versus)}` : ""
            }`}
          />
          <NightBar
            periods={night.periods}
            usualBed={night.usualBed}
            usualWake={night.usualWake}
            sentence={nightSentence(night)}
            width={300}
            compact
          />
          <More kind="sleep" />
        </div>
      ) : null}

      {resting && restingNow ? (
        <div className={styles.item}>
          <Head
            label={kindName("restingHeartRate")}
            say={
              resting.usual
                ? `${restingNow.value} · ${usualText(resting.usual).toLowerCase()}`
                : String(restingNow.value)
            }
          />
          <BandLine
            points={resting.points}
            from={resting.from}
            to={resting.to}
            band={resting.usual}
            min={40}
            max={Math.max(80, ...resting.points.map((p) => p.value + 8))}
            journalDays={resting.journalDays}
            sentence={lineSentence({
              label: kindName("restingHeartRate"),
              value: String(restingNow.value),
              day: restingNow.day,
              today,
              band: resting.usual,
            })}
            width={300}
            height={84}
            labels={axisLabels(
              resting,
              isToday ? t("journal.health.todayShort") : shortDay(resting.to),
            )}
          />
          {resting.usual ? null : <p className={styles.waiting}>{usualWaiting()}</p>}
          <More kind="restingHeartRate" />
        </div>
      ) : null}

      {steps && stepsNow ? (
        <div className={styles.item}>
          <Head
            label={kindName("steps")}
            say={
              isToday
                ? t("journal.health.stepsToday", { steps: num(stepsNow.value) })
                : num(stepsNow.value)
            }
          />
          <DayBars
            points={steps.points}
            from={steps.from}
            to={steps.to}
            journalDays={steps.journalDays}
            sentence={stepsSentence({
              steps: stepsNow.value,
              day: stepsNow.day,
              today,
              average: stepAverage,
              days: steps.points.length,
            })}
            width={300}
            height={70}
          />
          <More kind="steps" />
        </div>
      ) : null}

      {heart ? (
        <div className={styles.item}>
          <Head
            label={kindName("heartRate")}
            say={t("journal.health.rangeSay", {
              low: num(heart.low),
              high: num(heart.high),
              middle: num(heart.middle),
            })}
          />
          <DayRange
            low={heart.low}
            high={heart.high}
            middle={heart.middle}
            sentence={rangeSentence({ ...heart, day, today })}
            width={300}
          />
          <More kind="heartRate" />
        </div>
      ) : null}

      {pressure.length > 0 ? (
        <div className={styles.item}>
          {pressure.map((p) => (
            <Head
              key={p.itemId}
              label={kindName("bloodPressure")}
              say={`${pressureText(p.top, p.bottom)} · ${clock12(p.at)}`}
            />
          ))}
          <More kind="bloodPressure" />
        </div>
      ) : null}
    </section>
  );
}
