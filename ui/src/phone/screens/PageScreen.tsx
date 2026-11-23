import { type RefObject, useEffect, useRef, useState } from "react";
import { richText } from "../../components/PageLine/richText";
import { receiptLabel, receiptText } from "../../components/ReceiptChip/ReceiptChip";
import { formatLongDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { PageLine, PageSummary, PageView, Receipt } from "../../engine/bindings";
import { ConnectionError, type PhoneEngine, PhoneRequestError, type Queue } from "../engine";
import { PageQuestion } from "./PageQuestion";
import { CalendarToday, LastNight } from "./PageSections";
import { BackBar, Button, Page } from "./ui";

const chipFormat = new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric" });

/** "Wed 12" from a `YYYY-MM-DD` day. */
export function chipLabel(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  if (!y || !m || !d) return date;
  return chipFormat.format(new Date(y, m - 1, d, 12)).replace(",", "");
}

function Lines({ lines, onOpen }: { lines: PageLine[]; onOpen(r: Receipt): void }) {
  return (
    <>
      {lines.map((line) => (
        <div key={line.id}>
          <p className="ph-line">{richText(line.text)}</p>
          {line.receipts.length > 0 && (
            <span className="ph-srcs">
              {line.receipts.map((r) => (
                <button
                  key={`${line.id}-${r.itemId}`}
                  type="button"
                  className="ph-src"
                  aria-label={receiptLabel(r)}
                  onClick={() => onOpen(r)}
                >
                  {receiptText(r)}
                </button>
              ))}
            </span>
          )}
          {line.kind === "which" && (
            <p className="ph-hint ph-hint--ink2">{t("phone.page.which")}</p>
          )}
        </div>
      ))}
    </>
  );
}

export type PageLoad =
  | { name: "loading" }
  | { name: "ready"; page: PageView }
  | { name: "none" }
  | { name: "failed" };

/** A page by its day, today when none is given; `attempt` asks again. */
export function usePage(engine: PhoneEngine, date: string | undefined, attempt: number): PageLoad {
  const [load, setLoad] = useState<PageLoad>({ name: "loading" });
  useEffect(() => {
    void attempt;
    let gone = false;
    setLoad({ name: "loading" });
    engine.getPage(date).then(
      (page) => !gone && setLoad({ name: "ready", page }),
      (e: unknown) => {
        if (gone) return;
        if (e instanceof PhoneRequestError && (e.code === "notFound" || e.code === "notReady"))
          return setLoad({ name: "none" });
        if (e instanceof PhoneRequestError || e instanceof ConnectionError)
          return setLoad({ name: "failed" });
        throw e;
      },
    );
    return () => {
      gone = true;
    };
  }, [engine, date, attempt]);
  return load;
}

/** The pages before this one, newest first. */
export function useEarlier(engine: PhoneEngine): PageSummary[] {
  const [earlier, setEarlier] = useState<PageSummary[]>([]);
  useEffect(() => {
    let gone = false;
    engine.listPages().then(
      (pages) => !gone && setEarlier(pages),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [engine]);
  return earlier;
}

/** A receipt for an appointment or a night the page names, so it opens like any other. */
function phoneReceipt(itemId: string, kind: "calendar" | "reading", at: string): Receipt {
  return {
    itemId,
    kind,
    source: "phone",
    label: "",
    keptAt: at,
    durationMs: null,
    span: null,
    unconfirmed: false,
    fromDevice: null,
  };
}

interface ArticleProps {
  page: PageView;
  engine: PhoneEngine;
  queue: Queue;
  connected: boolean;
  earlier: readonly PageSummary[];
  now(): number;
  onQueued(): void;
  onOpen(r: Receipt): void;
  onJournal(day: string | null): void;
  heading: RefObject<HTMLHeadingElement | null>;
  /** The app: last night and the calendar, which the phone page in a browser leaves out. */
  phone?: boolean;
  /** Under the title: where the page was written, on a phone alone with an online mind. */
  byline?: string;
}

/** The phone edition of a page: the title, last night, the calendar, then the page's sections. */
export function PageArticle({
  page,
  engine,
  queue,
  connected,
  earlier,
  now,
  onQueued,
  onOpen,
  onJournal,
  heading,
  phone = false,
  byline,
}: ArticleProps) {
  return (
    <article className="ph-article">
      <span className="ph-lbl">{formatLongDay(page.date)}</span>
      <h2 tabIndex={-1} ref={heading}>
        {page.headline}
      </h2>
      {byline ? <p className="ph-byline">{byline}</p> : null}
      {page.dek && <p className="ph-dek">{page.dek}</p>}
      {phone && page.lastNight ? (
        <LastNight
          night={page.lastNight}
          onOpen={(id) => onOpen(phoneReceipt(id, "reading", page.lastNight?.to ?? page.date))}
        />
      ) : null}
      {phone && page.calendarToday && page.calendarToday.length > 0 ? (
        <CalendarToday
          lines={page.calendarToday}
          more={page.moreCalendarToday ?? 0}
          onOpen={(id) => onOpen(phoneReceipt(id, "calendar", page.date))}
          onMore={() => onJournal(page.date)}
        />
      ) : null}
      {page.needsYou.length > 0 && (
        <section className="ph-section">
          <h3 className="ph-lbl ph-lbl--ember">
            {t("phone.page.needsYou", { count: page.needsYou.length })}
          </h3>
          <Lines lines={page.needsYou} onOpen={onOpen} />
        </section>
      )}
      {page.changed.length > 0 && (
        <section className="ph-section" style={{ marginTop: 10 }}>
          <h3 className="ph-lbl">{t("phone.page.changed", { count: page.changed.length })}</h3>
          <Lines lines={page.changed} onOpen={onOpen} />
        </section>
      )}
      {page.upcoming.length > 0 && (
        <section className="ph-section" style={{ marginTop: 10 }}>
          <h3 className="ph-lbl">{t("phone.page.upcoming", { count: page.upcoming.length })}</h3>
          <Lines lines={page.upcoming} onOpen={onOpen} />
        </section>
      )}
      {page.needsYou.length === 0 && page.changed.length === 0 && (
        <p className="ph-body">{t("phone.page.quiet")}</p>
      )}
      {page.question?.kind === "journal" && page.question.journal && (
        <PageQuestion
          key={page.date}
          engine={engine}
          queue={queue}
          connected={connected}
          pageDate={page.date}
          question={page.question}
          latest={!earlier.some((p) => p.date > page.date)}
          now={now}
          onQueued={onQueued}
          onOpenDay={(day) => onJournal(day)}
          onOpenJournal={() => onJournal(null)}
        />
      )}
      {page.pending > 0 && (
        <p className="ph-hint">{t("phone.page.pending", { count: page.pending })}</p>
      )}
    </article>
  );
}

/** "Earlier pages": the seven before this one, as chips. */
export function EarlierChips({
  earlier,
  shown,
  onPick,
}: {
  earlier: readonly PageSummary[];
  shown: string | undefined;
  onPick(date: string): void;
}) {
  if (earlier.length === 0) return null;
  return (
    <nav className="ph-chips" aria-label={t("phone.page.earlierLabel")}>
      <span style={{ fontSize: "0.875rem", fontWeight: 500, marginRight: 4 }}>
        {t("phone.page.earlier")}
      </span>
      {earlier.slice(0, 7).map((p) => (
        <button
          key={p.date}
          type="button"
          className="ph-chip"
          aria-current={shown === p.date ? true : undefined}
          onClick={() => onPick(p.date)}
        >
          {chipLabel(p.date)}
        </button>
      ))}
    </nav>
  );
}

interface PageProps {
  engine: PhoneEngine;
  queue: Queue;
  connected: boolean;
  now(): number;
  onQueued(): void;
  onOpen(r: Receipt): void;
  /** Opens a day of the journal; today when none is given. */
  onJournal(day: string | null): void;
  onBack(): void;
}

/** P4: today's page in the phone edition, with the seven pages before it. */
export function PageScreen({
  engine,
  queue,
  connected,
  now,
  onQueued,
  onOpen,
  onJournal,
  onBack,
}: PageProps) {
  const [date, setDate] = useState<string | undefined>(undefined);
  const [attempt, setAttempt] = useState(0);
  const load = usePage(engine, date, attempt);
  const earlier = useEarlier(engine);
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (load.name !== "loading") heading.current?.focus({ preventScroll: true });
  }, [load.name]);

  const shown = load.name === "ready" ? load.page.date : date;
  return (
    <Page bar={<BackBar title={t("phone.page.title")} onBack={onBack} />} fixed>
      {load.name === "loading" && (
        <main className="ph-main" aria-busy="true">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {t("phone.page.loading")}
          </p>
        </main>
      )}
      {load.name === "failed" && (
        <main className="ph-main">
          <h1 className="ph-h1 ph-h1--small" role="alert" tabIndex={-1} ref={heading}>
            {t("phone.page.failed")}
          </h1>
          <div className="ph-spacer">
            <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
              {t("phone.tryAgain")}
            </Button>
          </div>
        </main>
      )}
      {load.name === "none" && (
        <main className="ph-main">
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {t("phone.page.none")}
          </h1>
          <p className="ph-body">{t("phone.page.none.body")}</p>
        </main>
      )}
      {load.name === "ready" && (
        <main className="ph-paper">
          <PageArticle
            page={load.page}
            engine={engine}
            queue={queue}
            connected={connected}
            earlier={earlier}
            now={now}
            onQueued={onQueued}
            onOpen={onOpen}
            onJournal={onJournal}
            heading={heading}
          />
        </main>
      )}
      <footer className="ph-foot">
        <span className="ph-rc">{t("phone.page.edition")}</span>
        <EarlierChips earlier={earlier} shown={shown} onPick={setDate} />
      </footer>
    </Page>
  );
}
