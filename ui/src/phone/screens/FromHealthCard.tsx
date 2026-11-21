import { BandLine } from "../../components/health/BandLine";
import { DayBars } from "../../components/health/DayBars";
import { DayRange } from "../../components/health/DayRange";
import { NightBar } from "../../components/health/NightBar";
import { ShowAsList } from "../../components/health/ShowAsList";
import {
  dayLabel,
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
import { formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type { DayHealth, HealthSeries } from "../../engine/bindings";

const WIDTH = 320;

const lastOf = (series: HealthSeries) => series.points[series.points.length - 1];

function Head({ label, say }: { label: string; say: string }) {
  return (
    <div className="ph-health-head">
      <span className="ph-health-name">{label}</span>
      <span className="ph-health-say">{say}</span>
    </div>
  );
}

/**
 * The days of a series as a list. A reading has no page of its own on the phone either: each
 * day opens that day of the journal, where its readings sit.
 */
function listOf(series: HealthSeries, value: string, onDay: (day: string) => void) {
  return {
    columns: [t("health.list.day"), value],
    rows: series.points.map((p) => ({
      key: p.itemId,
      cells: [
        <button key="day" type="button" className="ph-link" onClick={() => onDay(p.day)}>
          {dayLabel(p.day)}
        </button>,
        num(p.value),
      ],
    })),
  };
}

/** "From Health" after the timeline: the night, resting heart rate, steps, heart rate, pressure. */
export function FromHealthCard({
  health,
  day,
  isToday,
  onDay,
}: {
  health: DayHealth;
  day: string;
  isToday: boolean;
  /** A day in a list opens that day of the journal. */
  onDay(day: string): void;
}) {
  const { night, resting, steps, heart, pressure } = health;
  if (!night && !resting && !steps && !heart && pressure.length === 0) return null;
  const today = isToday ? day : "";
  const restingNow = resting ? lastOf(resting) : undefined;
  const stepsNow = steps ? lastOf(steps) : undefined;
  const stepAverage = steps
    ? steps.points.reduce((sum, p) => sum + p.value, 0) / Math.max(1, steps.points.length)
    : 0;
  const restingSentence =
    resting && restingNow
      ? lineSentence({
          label: kindName("restingHeartRate"),
          value: t("health.beats", { value: restingNow.value }),
          day: restingNow.day,
          today,
          band: resting.usual,
        })
      : "";
  const stepsSay =
    steps && stepsNow
      ? stepsSentence({
          steps: stepsNow.value,
          day: stepsNow.day,
          today,
          average: stepAverage,
          days: steps.points.length,
        })
      : "";
  return (
    <section className="ph-card ph-health" aria-labelledby="ph-health-label">
      <h2 className="ph-lbl" id="ph-health-label" style={{ margin: 0 }}>
        {t("health.fromHealth")}
      </h2>
      {night ? (
        <div className="ph-health-item">
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
            width={WIDTH}
            compact
          />
        </div>
      ) : null}
      {resting && restingNow ? (
        <div className="ph-health-item">
          <Head
            label={kindName("restingHeartRate")}
            say={
              resting.usual
                ? `${restingNow.value} · ${usualText(resting.usual).toLowerCase()}`
                : String(restingNow.value)
            }
          />
          <ShowAsList
            sentence={restingSentence}
            chart={
              <BandLine
                points={resting.points}
                from={resting.from}
                to={resting.to}
                band={resting.usual}
                min={40}
                max={Math.max(80, ...resting.points.map((p) => p.value + 8))}
                journalDays={resting.journalDays}
                sentence={restingSentence}
                width={WIDTH}
                height={84}
              />
            }
            {...listOf(resting, t("health.list.value.beats"), onDay)}
          />
          {resting.usual ? null : <p className="ph-hint">{usualWaiting()}</p>}
        </div>
      ) : null}
      {steps && stepsNow ? (
        <div className="ph-health-item">
          <Head
            label={kindName("steps")}
            say={
              isToday
                ? t("journal.health.stepsToday", { steps: num(stepsNow.value) })
                : num(stepsNow.value)
            }
          />
          <ShowAsList
            sentence={stepsSay}
            chart={
              <DayBars
                points={steps.points}
                from={steps.from}
                to={steps.to}
                journalDays={steps.journalDays}
                sentence={stepsSay}
                width={WIDTH}
                height={70}
              />
            }
            {...listOf(steps, t("health.list.value.steps"), onDay)}
          />
        </div>
      ) : null}
      {heart ? (
        <div className="ph-health-item">
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
            width={WIDTH}
          />
        </div>
      ) : null}
      {pressure.length > 0 ? (
        <div className="ph-health-item">
          {pressure.map((p) => (
            <Head
              key={p.itemId}
              label={kindName("bloodPressure")}
              say={`${pressureText(p.top, p.bottom)} · ${formatTime12(p.at)}`}
            />
          ))}
        </div>
      ) : null}
    </section>
  );
}
