import { type ReactNode, useEffect, useState } from "react";
import { formatLongDay, formatTime12 } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { HomeStats, JournalDay, PageView, TimelineRow } from "../../../engine/bindings";
import type { PhoneEngine } from "../../engine";
import { CalendarToday, LastNight } from "../../screens/PageSections";
import { Button, Page } from "../../screens/ui";
import { KeptLine } from "./kept";
import { chips } from "./parts";

/** Rows that are things kept, not plans or readings the phone brought. */
const KEPT: ReadonlySet<TimelineRow["kind"]> = new Set([
  "note",
  "voice",
  "photo",
  "good",
  "elsewhere",
  "answer",
]);

type Load =
  | { name: "loading" }
  | { name: "ready"; day: JournalDay; page: PageView | null; calendarOn: boolean }
  | { name: "failed" };

export interface HomeTodayProps {
  engine: PhoneEngine;
  bar: ReactNode;
  /** Whether the calendar is brought, so an empty one can say so. */
  calendarOn(): Promise<boolean>;
  stats(): Promise<HomeStats>;
  /** Whether the lost-phone reminder is due, from the memory's numbers. */
  reminderDue(stats: HomeStats): Promise<boolean>;
  onReminder(save: boolean): void;
  onMind(): void;
  onCheckIn(): void;
  onItem(id: string): void;
  onCalendarItem(id: string): void;
  /** Last night opens its day in the journal, from when the night ended. */
  onNight(to: string): void;
  onJournal(): void;
}

/** Today on a phone alone: the no-mind card where the page would be, then what this phone holds. */
export function HomeToday(p: HomeTodayProps) {
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [reminder, setReminder] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const { engine, calendarOn, stats, reminderDue } = p;

  useEffect(() => {
    void attempt;
    let gone = false;
    (async () => {
      try {
        const [day, page, on] = await Promise.all([
          engine.getJournalDay(),
          engine.getPage().catch(() => null),
          calendarOn().catch(() => false),
        ]);
        if (!gone) setLoad({ name: "ready", day, page, calendarOn: on });
      } catch {
        if (!gone) setLoad({ name: "failed" });
      }
    })();
    stats()
      .then(reminderDue)
      .then(
        (due) => !gone && setReminder(due),
        () => undefined,
      );
    return () => {
      gone = true;
    };
  }, [engine, calendarOn, stats, reminderDue, attempt]);

  const answer = (save: boolean) => {
    setReminder(false);
    p.onReminder(save);
  };

  let body: ReactNode = null;
  if (load.name === "failed") {
    body = (
      <>
        <h2 className="ph-h1 ph-h1--small" role="alert">
          {t("journal.timeline.error")}
        </h2>
        <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
          {t("phone.tryAgain")}
        </Button>
      </>
    );
  } else if (load.name === "ready") {
    const { day, page } = load;
    const kept = day.rows.filter((r) => KEPT.has(r.kind));
    const calendar = page?.calendarToday ?? [];
    body = (
      <>
        <p className="ph-lbl">{formatLongDay(day.day)}</p>
        {reminder ? (
          <div className="ph-reminder">
            <p>{t("phone.alone.reminder.body")}</p>
            <div className="ph-reminder-actions">
              <Button kind="primary" onClick={() => answer(true)}>
                {t("phone.alone.reminder.save")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={() => answer(false)}>
                {t("phone.alone.reminder.notNow")}
              </button>
            </div>
          </div>
        ) : null}
        <section className="ph-nomind" aria-labelledby="ph-nomind-title">
          <h2 id="ph-nomind-title">{t("phone.alone.noMind.title")}</h2>
          <p className="ph-body ph-ink2">{t("phone.alone.noMind.body")}</p>
          <button type="button" className="ph-btn ph-btn--tonal" onClick={p.onMind}>
            {t("phone.alone.noMind.choose")}
          </button>
        </section>
        {page?.lastNight ? (
          <LastNight night={page.lastNight} onOpen={() => p.onNight(page.lastNight?.to ?? "")} />
        ) : null}
        {calendar.length > 0 ? (
          <CalendarToday lines={calendar} more={0} onOpen={p.onCalendarItem} onMore={p.onJournal} />
        ) : load.calendarOn ? (
          <section className="ph-stack" style={{ gap: 4 }} aria-labelledby="ph-nocal">
            <h2 className="ph-lbl" id="ph-nocal">
              {t("phone.alone.today.calendar")}
            </h2>
            <p className="ph-small">{t("phone.alone.today.noCalendar")}</p>
          </section>
        ) : null}
        <button type="button" className="ph-feelcard" onClick={p.onCheckIn}>
          <span>{t("journal.checkin.title")}</span>
          <span className="ph-feelcard-act">{t("journal.side.checkIn")}</span>
        </button>
        {kept.length === 0 ? (
          <p className="ph-small">{chips("phone.alone.today.empty")}</p>
        ) : (
          <section className="ph-stack" style={{ gap: 4 }} aria-labelledby="ph-kept-today">
            <h2 className="ph-lbl" id="ph-kept-today">
              {t("phone.alone.today.kept", { count: kept.length })}
            </h2>
            <ul className="ph-kept-list">
              {kept.slice(0, 3).map((row) => (
                <li key={row.item.id}>
                  <button
                    type="button"
                    className="ph-kept-row"
                    onClick={() => p.onItem(row.item.id)}
                  >
                    <span className="ph-kept-at">{formatTime12(row.item.keptAt)}</span>
                    <KeptLine row={row} />
                  </button>
                </li>
              ))}
            </ul>
            <button type="button" className="ph-link" onClick={p.onJournal}>
              {t("phone.alone.today.seeJournal")}
            </button>
          </section>
        )}
      </>
    );
  }

  return (
    <Page bar={p.bar} className="ph-tabbed">
      <main className="ph-main ph-main--tight" aria-busy={load.name === "loading"}>
        <p className="ph-status" role="status">
          <span className="ph-dot" aria-hidden="true" />
          {t("phone.alone.today.status")}
        </p>
        {body}
      </main>
    </Page>
  );
}
