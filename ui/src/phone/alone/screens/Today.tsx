import { type ReactNode, useEffect, useRef, useState } from "react";
import { formatLongDay, formatTimeOfDay, isAfterMidnight } from "../../../copy/format";
import { t } from "../../../copy/t";
import type {
  HomeStats,
  JournalDay,
  PageView,
  Receipt,
  TimelineRow,
} from "../../../engine/bindings";
import type { PhoneEngine, Queue } from "../../engine";
import { PageArticle } from "../../screens/PageScreen";
import { CalendarToday, LastNight } from "../../screens/PageSections";
import { Button, Page } from "../../screens/ui";
import type { MindTrouble } from "../mind";
import { KeptLine } from "./kept";
import { MindNotice, Offline, UnreachableToday } from "./MindNotice";
import { chips, NearlyFull } from "./parts";

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
  now(): number;
  /** Whether the calendar is brought, so an empty one can say so. */
  calendarOn(): Promise<boolean>;
  stats(): Promise<HomeStats>;
  /** Whether the lost-phone reminder is due, from the memory's numbers. */
  reminderDue(stats: HomeStats): Promise<boolean>;
  onReminder(save: boolean): void;
  /** The backup's own card: stopped, or no backup for seven nights. Shown once for each cause. */
  backupCard?: BackupCard | null;
  /** "Open backup" (true) or "Not now". */
  onBackupCard?(open: boolean): void;
  onMind(): void;
  onCheckIn(): void;
  onItem(id: string): void;
  onCalendarItem(id: string): void;
  /** Last night opens its day in the journal, from when the night ended. */
  onNight(to: string): void;
  onJournal(): void;
  /** Less than 500 MB is free: the banner shows under the status. */
  nearlyFull: boolean;
  /**
   * The chosen online mind's host, null with no mind. With one, the no-mind card goes, and a
   * page it wrote shows in its place.
   */
  mind: string | null;
  /** How the chosen service is doing; a trouble shows over the page, with the way to fix it. */
  status?: MindTrouble | null;
  /** "Check the key…", "Check again" and "Change limit". */
  onFixMind?(): void;
  queue: Queue;
  onQueued(): void;
  /** A receipt on the page's lines. */
  onOpen(receipt: Receipt): void;
  /** The page's question opens the journal, on its day or today's. */
  onJournalDay(day: string | null): void;
}

export interface BackupCard {
  /** "Your backup has stopped.", for a stop only the person can fix. */
  stopped: boolean;
  line: string;
}

/** A page the mind wrote: one with a title. A phone with no mind has none. */
export function written(page: PageView | null): page is PageView {
  return page !== null && page.headline.trim() !== "";
}

/** Today on a phone alone: the no-mind card where the page would be, then what this phone holds. */
export function HomeToday(p: HomeTodayProps) {
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [reminder, setReminder] = useState(false);
  const [cardGone, setCardGone] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const { engine, calendarOn, stats, reminderDue } = p;
  const heading = useRef<HTMLHeadingElement>(null);

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
        {p.mind !== null && written(page) ? null : (
          <p className="ph-lbl">
            {isAfterMidnight(new Date(p.now()))
              ? t("phone.alone.today.afterMidnight", { day: formatLongDay(day.day) })
              : formatLongDay(day.day)}
          </p>
        )}
        {p.backupCard && !cardGone ? (
          <section className="ph-reminder" aria-label={t("phone.backup.row")}>
            {p.backupCard.stopped ? (
              <>
                <strong className="ph-nb-cardtitle">{t("phone.backup.card.stopped")}</strong>
                <p>{p.backupCard.line}</p>
              </>
            ) : (
              <p>{p.backupCard.line}</p>
            )}
            <div className="ph-reminder-actions">
              <Button
                kind="primary"
                onClick={() => {
                  setCardGone(true);
                  p.onBackupCard?.(true);
                }}
              >
                {t("phone.backup.card.open")}
              </Button>
              <button
                type="button"
                className="ph-textbtn"
                onClick={() => {
                  setCardGone(true);
                  p.onBackupCard?.(false);
                }}
              >
                {t("phone.backup.notNow")}
              </button>
            </div>
          </section>
        ) : null}
        {reminder ? (
          <div className="ph-reminder">
            <p>{t("phone.backup.card.none")}</p>
            <div className="ph-reminder-actions">
              <Button kind="primary" onClick={() => answer(true)}>
                {t("phone.backup.card.turnOn")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={() => answer(false)}>
                {t("phone.alone.reminder.notNow")}
              </button>
            </div>
          </div>
        ) : null}
        {p.status && p.onFixMind ? <MindNotice status={p.status} onFix={p.onFixMind} /> : null}
        {p.status?.state === "offline" ? <Offline /> : null}
        {p.mind === null ? (
          <section className="ph-nomind" aria-labelledby="ph-nomind-title">
            <h2 id="ph-nomind-title">{t("phone.alone.noMind.title")}</h2>
            <p className="ph-body ph-ink2">{t("phone.alone.noMind.body")}</p>
            <button type="button" className="ph-btn ph-btn--tonal" onClick={p.onMind}>
              {t("phone.alone.noMind.choose")}
            </button>
          </section>
        ) : null}
        {p.mind !== null && written(page) ? (
          <div className="ph-alone-page">
            <PageArticle
              page={page}
              engine={engine}
              queue={p.queue}
              connected
              earlier={[]}
              now={p.now}
              onQueued={p.onQueued}
              onOpen={p.onOpen}
              onJournal={p.onJournalDay}
              heading={heading}
              phone
              byline={t("phone.mind.today.written", { host: p.mind })}
            />
          </div>
        ) : null}
        {p.status?.state === "unreachable" && written(page) ? (
          <UnreachableToday status={p.status} written={page.date} />
        ) : null}
        {written(page) && p.mind !== null ? null : page?.lastNight ? (
          <LastNight night={page.lastNight} onOpen={() => p.onNight(page.lastNight?.to ?? "")} />
        ) : null}
        {written(page) && p.mind !== null ? null : calendar.length > 0 ? (
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
                    <span className="ph-kept-at">{formatTimeOfDay(row.item.keptAt)}</span>
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
        {p.nearlyFull ? <NearlyFull /> : null}
        {body}
      </main>
    </Page>
  );
}
