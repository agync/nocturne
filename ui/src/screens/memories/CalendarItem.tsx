import { Icon } from "../../components/Icon/Icon";
import { formatLongDay, formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type { CalendarDetail } from "../../engine/bindings";
import styles from "./Item.module.css";

/** "4:30 pm to 5:15 pm", "all day", with the entry's own place name when its zone differs. */
function span(detail: CalendarDetail): string {
  if (detail.allDay) return t("item.calendar.allDay");
  const range = t("item.calendar.range", {
    from: formatTime12(detail.start),
    to: formatTime12(detail.end),
  });
  return detail.zone ? t("page.calendar.zone", { time: range, place: detail.zone }) : range;
}

/** "Friday 20 November, 4:30 pm to 5:15 pm". */
export function whenText(detail: CalendarDetail): string {
  return t("item.calendar.when", { day: formatLongDay(detail.start), span: span(detail) });
}

/** The left of a calendar entry's page: the mark, the title, and when. */
export function CalendarPreview({ title, detail }: { title: string; detail: CalendarDetail }) {
  return (
    <div className={styles.calendarPreview}>
      <Icon name="calendar" size={48} />
      <p className={styles.calendarQuote}>{title}</p>
      <p className={styles.calendarWhen}>{whenText(detail)}</p>
    </div>
  );
}

/** When, where and who, then what changed: everything the phone said about the entry. */
export function CalendarFacts({ detail }: { detail: CalendarDetail }) {
  return (
    <>
      <ul className={styles.facts}>
        <li>
          {whenText(detail)}
          {detail.repeats ? <small>{detail.repeatRule ?? t("item.calendar.repeats")}</small> : null}
        </li>
        {detail.place ? <li>{detail.place}</li> : null}
        {detail.people.length > 0 ? (
          <li>
            {detail.people.join(", ")}
            <small>{t("item.calendar.invited")}</small>
          </li>
        ) : null}
      </ul>
      <p className={styles.calendarFrom}>
        {detail.removed
          ? t("item.calendar.removed")
          : t("item.calendar.from", { phone: detail.phone, time: formatTime12(detail.updatedAt) })}
      </p>
      {detail.was ? (
        <section className={styles.from} aria-labelledby="versions">
          <h2 id="versions" className="t-section">
            {t("item.calendar.versions")}
          </h2>
          <p className={styles.none}>
            {t("item.calendar.was", {
              when: t("item.calendar.when", {
                day: formatLongDay(detail.was.start),
                span: formatTime12(detail.was.start),
              }),
            })}
          </p>
        </section>
      ) : null}
    </>
  );
}
