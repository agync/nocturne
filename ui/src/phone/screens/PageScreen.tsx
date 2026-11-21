import { useEffect, useRef, useState } from "react";
import { richText } from "../../components/PageLine/richText";
import { receiptLabel, receiptText } from "../../components/ReceiptChip/ReceiptChip";
import { formatLongDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { PageLine, PageSummary, PageView, Receipt } from "../../engine/bindings";
import { ConnectionError, type PhoneEngine, PhoneRequestError, type Queue } from "../engine";
import { PageQuestion } from "./PageQuestion";
import { BackBar, Button, Page } from "./ui";

const chipFormat = new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric" });

/** "Wed 12" from a `YYYY-MM-DD` day. */
export function chipLabel(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  if (!y || !m || !d) return date;
  return chipFormat.format(new Date(y, m - 1, d, 12)).replace(",", "");
}

type Load =
  | { name: "loading" }
  | { name: "ready"; page: PageView }
  | { name: "none" }
  | { name: "failed" };

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
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [earlier, setEarlier] = useState<PageSummary[]>([]);
  const [attempt, setAttempt] = useState(0);
  const heading = useRef<HTMLHeadingElement>(null);

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
          <article className="ph-article">
            <span className="ph-lbl">{formatLongDay(load.page.date)}</span>
            <h2 tabIndex={-1} ref={heading}>
              {load.page.headline}
            </h2>
            {load.page.dek && <p className="ph-dek">{load.page.dek}</p>}
            {load.page.needsYou.length > 0 && (
              <section className="ph-section">
                <h3 className="ph-lbl ph-lbl--ember">
                  {t("phone.page.needsYou", { count: load.page.needsYou.length })}
                </h3>
                <Lines lines={load.page.needsYou} onOpen={onOpen} />
              </section>
            )}
            {load.page.changed.length > 0 && (
              <section className="ph-section" style={{ marginTop: 10 }}>
                <h3 className="ph-lbl">
                  {t("phone.page.changed", { count: load.page.changed.length })}
                </h3>
                <Lines lines={load.page.changed} onOpen={onOpen} />
              </section>
            )}
            {load.page.upcoming.length > 0 && (
              <section className="ph-section" style={{ marginTop: 10 }}>
                <h3 className="ph-lbl">
                  {t("phone.page.upcoming", { count: load.page.upcoming.length })}
                </h3>
                <Lines lines={load.page.upcoming} onOpen={onOpen} />
              </section>
            )}
            {load.page.needsYou.length === 0 && load.page.changed.length === 0 && (
              <p className="ph-body">{t("phone.page.quiet")}</p>
            )}
            {load.page.question?.kind === "journal" && load.page.question.journal && (
              <PageQuestion
                key={load.page.date}
                engine={engine}
                queue={queue}
                connected={connected}
                pageDate={load.page.date}
                question={load.page.question}
                latest={!earlier.some((p) => p.date > load.page.date)}
                now={now}
                onQueued={onQueued}
                onOpenDay={(day) => onJournal(day)}
                onOpenJournal={() => onJournal(null)}
              />
            )}
            {load.page.pending > 0 && (
              <p className="ph-hint">{t("phone.page.pending", { count: load.page.pending })}</p>
            )}
          </article>
        </main>
      )}
      <footer className="ph-foot">
        <span className="ph-rc">{t("phone.page.edition")}</span>
        {earlier.length > 0 && (
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
                onClick={() => setDate(p.date)}
              >
                {chipLabel(p.date)}
              </button>
            ))}
          </nav>
        )}
      </footer>
    </Page>
  );
}
