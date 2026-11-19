import { addDays, formatWeekday } from "../../copy/format";
import { t } from "../../copy/t";
import type { CheckIn, FeelingPart, Period } from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import { PART_ORDER } from "../../journal/feelings";
import styles from "./Lookback.module.css";
import {
  checkinLabel,
  clock,
  dayOf,
  itemHref,
  monthName,
  partName,
  weekdayDay,
  wordsOf,
} from "./text";

const FILL: Record<FeelingPart, string> = {
  woundUp: "var(--part-wound)",
  litUp: "var(--part-lit)",
  low: "var(--part-low)",
  settled: "var(--part-settled)",
};

/** A small field that grows with its column, with a dot for each check-in that opens it. */
export function MiniField({ checkins, label }: { checkins: readonly CheckIn[]; label: string }) {
  return (
    <div className={styles.mini}>
      {PART_ORDER.map((part) => (
        <span key={part} className={styles.miniPart} style={{ background: FILL[part] }} />
      ))}
      {checkins.length > 0 ? (
        <ul className={styles.miniDots} aria-label={label}>
          {checkins.map((c) => (
            <li
              key={c.id}
              style={{
                left: `${((c.feeling.point.pleasant + 1) / 2) * 100}%`,
                top: `${((1 - c.feeling.point.energy) / 2) * 100}%`,
              }}
            >
              <a
                className={styles.miniDot}
                href={itemHref(c.id)}
                aria-label={checkinLabel(c)}
                data-part={c.feeling.part}
              >
                <FeelingMark mark={c.feeling.part} size={10} />
              </a>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

const MONTHS = Array.from({ length: 12 }, (_, i) => i);

/** Seven small fields, Monday to Sunday. */
export function WeekChart({ period, checkins }: { period: Period; checkins: readonly CheckIn[] }) {
  const days = Array.from({ length: 7 }, (_, i) => addDays(period.start, i));
  return (
    <ul className={styles.week} aria-label={t("journal.lookback.chart.week")}>
      {days.map((day) => (
        <li key={day} className={styles.cell}>
          <MiniField checkins={checkins.filter((c) => c.day === day)} label={weekdayDay(day)} />
          <span className={styles.cellLabel}>{formatWeekday(day)}</span>
        </li>
      ))}
    </ul>
  );
}

/** Twelve small fields, one per month. */
export function YearChart({ period, checkins }: { period: Period; checkins: readonly CheckIn[] }) {
  const year = period.start.slice(0, 4);
  return (
    <ul className={styles.year} aria-label={t("journal.lookback.chart.year")}>
      {MONTHS.map((m) => {
        const first = `${year}-${String(m + 1).padStart(2, "0")}-01`;
        return (
          <li key={first} className={styles.cell}>
            <MiniField
              checkins={checkins.filter((c) => c.day.startsWith(first.slice(0, 7)))}
              label={monthName(first)}
            />
            <span className={styles.cellLabel}>{monthName(first, "short")}</span>
          </li>
        );
      })}
    </ul>
  );
}

/** A month as a calendar, Monday first, with each check-in a dot in its day. */
export function MonthChart({ period, checkins }: { period: Period; checkins: readonly CheckIn[] }) {
  const first = new Date(`${period.start}T12:00:00`);
  const lead = (first.getDay() + 6) % 7;
  const count = Number(period.end.slice(8, 10));
  const prefix = period.start.slice(0, 8);
  const monday = addDays(period.start, -lead);
  return (
    <div className={styles.monthWrap}>
      <div className={styles.dowRow} aria-hidden="true">
        {Array.from({ length: 7 }, (_, i) => (
          <span key={addDays(monday, i)} className={styles.dow}>
            {formatWeekday(addDays(monday, i))}
          </span>
        ))}
      </div>
      <ol className={styles.month} aria-label={t("journal.lookback.chart.month")}>
        {Array.from({ length: lead }, (_, i) => (
          <li key={`blank-${prefix}${i}`} className={styles.blank} aria-hidden="true" />
        ))}
        {Array.from({ length: count }, (_, i) => {
          const day = `${prefix}${String(i + 1).padStart(2, "0")}`;
          const here = checkins.filter((c) => c.day === day);
          return (
            <li key={day} className={styles.day}>
              <span>{i + 1}</span>
              {here.map((c) => (
                <a
                  key={c.id}
                  className={styles.monthDot}
                  href={itemHref(c.id)}
                  aria-label={`${weekdayDay(day)}, ${checkinLabel(c)}`}
                  data-part={c.feeling.part}
                >
                  <FeelingMark mark={c.feeling.part} size={10} />
                </a>
              ))}
            </li>
          );
        })}
      </ol>
    </div>
  );
}

/** The text alternative of the dots: every check-in with its day, time, part and words. */
export function CheckinTable({
  checkins,
  withDay = false,
}: {
  checkins: readonly CheckIn[];
  withDay?: boolean;
}) {
  return (
    <table className={styles.table}>
      <caption className="sr-only">{t("journal.lookback.table.checkins")}</caption>
      <thead>
        <tr>
          {withDay ? <th scope="col">{t("journal.lookback.table.day")}</th> : null}
          <th scope="col">{t("journal.lookback.table.time")}</th>
          <th scope="col">{t("journal.lookback.table.part")}</th>
          <th scope="col">{t("journal.lookback.table.words")}</th>
        </tr>
      </thead>
      <tbody>
        {checkins.map((c) => (
          <tr key={c.id}>
            {withDay ? <td>{dayLabel(c)}</td> : null}
            <td>
              <a href={itemHref(c.id)}>{clock(c.keptAt)}</a>
            </td>
            <td>
              <span className={styles.partCell} data-part={c.feeling.part}>
                <FeelingMark mark={c.feeling.part} size={12} />
                {partName(c.feeling.part)}
              </span>
            </td>
            <td>{wordsOf(c)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const dayLabel = (c: CheckIn) => `${formatWeekday(c.day)} ${Number(dayOf(c.keptAt).slice(8, 10))}`;
