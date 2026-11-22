import { NightBar } from "../../components/health/NightBar";
import { clock, duration, nightSentence, versusText } from "../../components/health/sentence";
import { formatTimeOfDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { CalendarLine, NightLine } from "../../engine/bindings";
import { calendarMeta, sleepMeta } from "../labels";
import { Icon } from "./ui";

/** "Last night": the night's length, its bar and when, with a receipt to the night. */
export function LastNight({ night, onOpen }: { night: NightLine; onOpen(itemId: string): void }) {
  const versus = versusText(night.versus);
  return (
    <section className="ph-night" aria-labelledby="ph-night-label">
      <div className="ph-night-head">
        <h3 className="ph-lbl" id="ph-night-label">
          {t("page.lastNight.label")}
        </h3>
        <button
          type="button"
          className="ph-chipsrc"
          aria-label={sleepMeta(night.phone)}
          onClick={() => onOpen(night.itemId)}
        >
          <Icon name="moon" size={12} />
          {t("phone.app.receipt.sleep")}
        </button>
      </div>
      <p className="ph-night-value">
        {t("journal.health.asleep", { asleep: duration(night.asleepMin) })}
        {versus ? <span className="ph-dim"> {t("page.lastNight.versus", { versus })}</span> : null}
      </p>
      <NightBar
        periods={night.periods}
        usualBed={night.usualBed}
        usualWake={night.usualWake}
        sentence={nightSentence(night)}
        width={340}
        compact
      />
      <p className="ph-hint ph-hint--ink2">
        {t("phone.app.today.night.when", {
          from: clock(night.from),
          to: clock(night.to),
          count: night.awakeCount,
        })}
      </p>
    </section>
  );
}

function at(line: CalendarLine): string {
  if (line.allDay) return t("page.calendar.allDay");
  const time = formatTimeOfDay(line.start);
  return line.zone ? t("page.calendar.zone", { time, place: line.zone }) : time;
}

/** "In your calendar today · 3": up to five rows, then "{count} more today" to the journal. */
export function CalendarToday({
  lines,
  more,
  onOpen,
  onMore,
}: {
  lines: readonly CalendarLine[];
  more: number;
  onOpen(itemId: string): void;
  onMore(): void;
}) {
  return (
    <section className="ph-section" aria-labelledby="ph-cal-label">
      <h3 className="ph-lbl" id="ph-cal-label">
        {t("page.section.calendarToday", { count: lines.length + more })}
      </h3>
      <ul className="ph-agenda">
        {lines.map((line) => (
          <li key={line.itemId}>
            <button type="button" className="ph-agenda-row" onClick={() => onOpen(line.itemId)}>
              <span className="sr-only">
                {t("phone.app.calendar.row", {
                  time: at(line),
                  title: line.title,
                  place: line.place ?? "none",
                  calendar: line.calendar,
                })}
              </span>
              <span className="ph-agenda-at" aria-hidden="true">
                {at(line)}
              </span>
              <span className="ph-agenda-what" aria-hidden="true">
                <span className="ph-agenda-title">{line.title}</span>
                {line.place ? <span className="ph-agenda-where">{line.place}</span> : null}
                <span className="ph-chipsrc">
                  <Icon name="calendar" size={12} />
                  {calendarMeta(line.calendar)}
                </span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      {more > 0 ? (
        <button type="button" className="ph-link" onClick={onMore}>
          {t("page.calendar.more", { count: more })}
        </button>
      ) : null}
    </section>
  );
}
