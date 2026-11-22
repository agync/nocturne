import { formatTimeOfDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { CalendarLine, NightLine, Receipt } from "../../engine/bindings";
import { NightBar } from "../health/NightBar";
import { nightLine, nightSentence, versusText } from "../health/sentence";
import { ReceiptChip } from "../ReceiptChip/ReceiptChip";
import styles from "./PhoneSections.module.css";

const receiptOf = (
  itemId: string,
  kind: "calendar" | "reading",
  label: string,
  keptAt: string,
  phone: string,
): Receipt => ({
  itemId,
  kind,
  source: "phone",
  label,
  keptAt,
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: phone,
});

/** "Last night": one line under the page's title, with the night bar and a receipt to the night. */
export function LastNight({
  id,
  night,
  onOpen,
}: {
  id: string;
  night: NightLine;
  onOpen: (receipt: Receipt) => void;
}) {
  const versus = versusText(night.versus);
  return (
    <section className={styles.night} aria-labelledby={id}>
      <div className={styles.nightText}>
        <h2 id={id} className={`t-section ${styles.label}`}>
          {t("page.lastNight.label")}
        </h2>
        <p className={styles.nightLine}>
          {nightLine(night)}
          {versus ? (
            <span className={styles.versus}> {t("page.lastNight.versus", { versus })}</span>
          ) : null}
        </p>
        <ReceiptChip
          receipt={receiptOf(night.itemId, "reading", night.phone, night.to, night.phone)}
          text={t("page.lastNight.receipt", { phone: night.phone })}
          icon="moon"
          onOpen={onOpen}
        />
      </div>
      <div className={styles.nightBar}>
        <NightBar
          periods={night.periods}
          usualBed={night.usualBed}
          usualWake={night.usualWake}
          sentence={nightSentence(night)}
          width={280}
          compact
        />
      </div>
    </section>
  );
}

function when(line: CalendarLine): string {
  if (line.allDay) return t("page.calendar.allDay");
  return formatTimeOfDay(line.start);
}

/** "In your calendar today · 3": the time, the title in the page's serif, the place, and the receipt. */
export function CalendarToday({
  id,
  lines,
  more,
  day,
  onOpen,
}: {
  id: string;
  lines: readonly CalendarLine[];
  more: number;
  day: string;
  onOpen: (receipt: Receipt) => void;
}) {
  return (
    <section className={styles.agenda} aria-labelledby={id}>
      <h2 id={id} className={`t-section ${styles.label}`}>
        {t("page.section.calendarToday", { count: lines.length + more })}
      </h2>
      <ul className={styles.rows}>
        {lines.map((line) => (
          <li key={line.itemId} className={styles.row}>
            <span className={styles.at}>
              {line.zone && !line.allDay
                ? t("page.calendar.zone", { time: formatTimeOfDay(line.start), place: line.zone })
                : when(line)}
            </span>
            <span className={styles.what}>
              <span className={styles.titleLine}>
                <span className={styles.title}>{line.title}</span>
                {line.place ? <span className={styles.place}> {line.place}</span> : null}
              </span>
              <span className={styles.chip}>
                <ReceiptChip
                  receipt={receiptOf(line.itemId, "calendar", line.title, line.start, line.phone)}
                  text={t("page.calendar.receipt", { calendar: line.calendar, phone: line.phone })}
                  onOpen={onOpen}
                />
              </span>
            </span>
          </li>
        ))}
      </ul>
      {more > 0 ? (
        <a className={styles.more} href={`#/journal/${day}`}>
          {t("page.calendar.more", { count: more })}
        </a>
      ) : null}
    </section>
  );
}
