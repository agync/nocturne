import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useRoute } from "wouter";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { richText } from "../../components/PageLine/richText";
import { opening } from "../../components/PageSheet/questionWords";
import { receiptText } from "../../components/ReceiptChip/ReceiptChip";
import { useToast } from "../../components/Toast/Toast";
import { formatLongDay, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { PageLine, PageView, Question } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { useEngine, usePage } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { type Fitted, fit, letGoName, type PaperSize } from "./fit";
import styles from "./Print.module.css";

const SIZE_KEY = "nocturne-print-size";
const SIZES: PaperSize[] = ["a4", "letter", "receipt"];

/** What the person chose last time, kept in this window's browser storage only. */
function savedSize(): PaperSize {
  try {
    const value = window.localStorage.getItem(SIZE_KEY);
    if (value === "a4" || value === "letter" || value === "receipt") return value;
  } catch {
    // Storage may be blocked; the default is A4 for en-GB.
  }
  return "a4";
}

function rememberSize(size: PaperSize): void {
  try {
    window.localStorage.setItem(SIZE_KEY, size);
  } catch {
    // Not remembering is fine.
  }
}

/** The sheet's height in millimetres, from what the browser laid out (96 pixels to the inch). */
export function pixelsToMm(px: number): number {
  return (px * 25.4) / 96;
}

/**
 * The `@page` rule for a size: the paper, and the margin the layout was fitted to. A receipt is
 * a roll: its page is as long as the sheet is, measured, so the printer does not feed blank
 * paper (`receiptMm` is that height, once it is known).
 */
export function pageRule(size: PaperSize, receiptMm?: number): string {
  switch (size) {
    case "a4":
      return "@page { size: A4; margin: 14mm; }";
    case "letter":
      return "@page { size: letter; margin: 0.6in; }";
    case "receipt":
      // The margins are inside the measured height; a little over keeps rounding from adding a page.
      return `@page { size: 80mm ${receiptMm ? Math.ceil(receiptMm) + 2 : 297}mm; margin: 4mm; }`;
  }
}

function Sources({ line }: { line: PageLine }) {
  return (
    <>
      {line.receipts.map((r) => (
        <span key={`${r.itemId}-${JSON.stringify(r.span)}`} className={styles.receipt}>
          {" "}
          [{receiptText(r)}]
        </span>
      ))}
    </>
  );
}

/** A line that asks which of two things is right prints its choices as tick circles. */
function PrintLine({ line }: { line: PageLine }) {
  return (
    <li className={styles.line} data-kind={line.kind}>
      <span className={styles.box} aria-hidden="true" />
      <div className={styles.lineBody}>
        <p className={`${styles.lineText} ${styles.needText}`}>
          {line.kind === "which" && line.subject
            ? t("print.which", { subject: line.subject })
            : richText(line.text)}
          {line.choices.length === 0 ? <Sources line={line} /> : null}
        </p>
        {line.choices.length > 0 ? (
          <ul className={styles.choices}>
            {line.choices.map((c) => (
              <li key={c.memoryId} className={styles.choice}>
                <span className={styles.circle} aria-hidden="true" />
                <span>
                  {richText(c.text)}{" "}
                  <span className={styles.receipt}>[{receiptText(c.receipt)}]</span>
                </span>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </li>
  );
}

function ChangedLine({ line }: { line: PageLine }) {
  return (
    <li className={styles.change}>
      <span className={styles.dash} aria-hidden="true" />
      <div className={styles.lineBody}>
        <p className={styles.lineText}>
          {richText(line.text)}
          <Sources line={line} />
        </p>
      </div>
    </li>
  );
}

function Part({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className={styles.part}>
      <h3 className={styles.partTitle}>{title}</h3>
      {children}
    </section>
  );
}

function questionText(question: Question): string {
  if (question.kind === "pageTime") return t("page.question.pageTime", { time: question.text });
  if (question.kind === "journal" && question.journal) {
    return [
      opening(question.journal),
      t("journal.question.quote", { sentence: question.text }),
      t("journal.question.howDidItGo"),
    ].join(" ");
  }
  return question.text;
}

function QuestionPart({ question }: { question: Question }) {
  const answers =
    question.kind === "pageTime"
      ? [t("page.question.earlier"), t("page.question.later")]
      : question.kind === "journal"
        ? [t("journal.question.fine"), t("journal.question.notGreat"), t("journal.question.notYet")]
        : [t("page.question.yes"), t("page.question.no"), t("page.question.notSure")];
  return (
    <Part title={t("print.question")}>
      <p className={styles.lineText}>{questionText(question)}</p>
      <ul className={styles.answers}>
        {answers.map((a) => (
          <li key={a} className={styles.choice}>
            <span className={styles.circle} aria-hidden="true" />
            <span>{a}</span>
          </li>
        ))}
      </ul>
    </Part>
  );
}

export interface SheetProps {
  page: PageView;
  size: PaperSize;
  fitted: Fitted;
  printedAt: string;
  /** True only when the amount sent was measured and is zero. */
  sentNothing: boolean;
  sheetRef?: React.Ref<HTMLElement>;
}

/** The sheet itself: black on white, no tint carries meaning, the source of each line in brackets. */
export function Sheet({ page, size, fitted, printedAt, sentNothing, sheetRef }: SheetProps) {
  const question = page.question && page.question.answered === null ? page.question : null;
  return (
    <article
      ref={sheetRef}
      className={styles.sheet}
      data-size={size}
      aria-label={t("screen.print.title")}
    >
      <header className={styles.masthead}>
        <p className={styles.brand}>{t("print.brand")}</p>
        <p className={styles.date}>
          {formatLongDay(page.date)} · {t("print.page", { number: page.pageNumber })}
        </p>
        <h2 className={styles.headline}>{page.headline}</h2>
        <p className={styles.dek}>{page.dek}</p>
      </header>
      {fitted.needsYou.length > 0 ? (
        <Part title={t("print.needsYou")}>
          <ul className={styles.lines}>
            {fitted.needsYou.map((line) => (
              <PrintLine key={line.id} line={line} />
            ))}
          </ul>
          {fitted.needsYouMore > 0 ? (
            <p className={styles.more}>{t("print.more", { count: fitted.needsYouMore })}</p>
          ) : null}
        </Part>
      ) : null}
      {question ? <QuestionPart question={question} /> : null}
      {fitted.changed.length > 0 ? (
        <Part title={t("print.changed")}>
          <ul className={styles.lines}>
            {fitted.changed.map((line) => (
              <ChangedLine key={line.id} line={line} />
            ))}
          </ul>
          {fitted.changedMore > 0 ? (
            <p className={styles.more}>{t("print.more", { count: fitted.changedMore })}</p>
          ) : null}
        </Part>
      ) : null}
      {fitted.letGo.length > 0 ? (
        <Part title={t("print.letGo")}>
          <p className={styles.lineText}>
            {fitted.letGo.map((line, index) => (
              <span key={line.id}>
                {index > 0 ? " · " : ""}
                {letGoName(line)}
                <Sources line={line} />
              </span>
            ))}
            {fitted.letGoMore > 0 ? ` · ${t("print.more", { count: fitted.letGoMore })}` : ""}
          </p>
        </Part>
      ) : null}
      <footer className={styles.footer}>
        <p>
          {t("print.footer.printed", { time: printedAt })}
          {page.privateSkipped > 0 ? ` ${t("print.footer.private")}` : ""}
        </p>
        {sentNothing ? <p>{t("print.footer.sent")}</p> : null}
      </footer>
    </article>
  );
}

/** Serves #/print/:date: the page for paper, on A4, Letter or an 80 mm receipt. */
export function Print() {
  const [, params] = useRoute("/print/:date");
  const date = params?.date ?? "";
  const today = usePage();
  const back =
    today.data?.date === date
      ? { to: "/today", label: t("screen.today.title") }
      : { to: `/pages/${date}`, label: t("screen.page.title") };
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const [size, setSize] = useState<PaperSize>(savedSize);
  const [printedAt] = useState(() => formatTime(new Date()));
  const sheet = useRef<HTMLElement>(null);
  const desk = useRef<HTMLDivElement>(null);
  /** On a window narrower than the paper the preview shrinks to fit, so it never scrolls sideways. */
  const [squeeze, setSqueeze] = useState<{ scale: number; width: number; height: number } | null>(
    null,
  );
  const [receiptMm, setReceiptMm] = useState<number | undefined>(undefined);

  const page = useQuery({
    queryKey: ["edition", date, "print"],
    queryFn: () => engine.getEdition(date, "print"),
    gcTime: 0,
    staleTime: 0,
  });
  const privacy = useQuery({ queryKey: ["privacy"], queryFn: () => engine.getPrivacy() });

  // A receipt's page is as long as its sheet: measure it, and again if it changes size.
  const ready = Boolean(page.data);
  useLayoutEffect(() => {
    const el = sheet.current;
    if (!ready || !el || size !== "receipt") return;
    const measure = () => setReceiptMm(pixelsToMm(el.offsetHeight));
    measure();
    void document.fonts?.ready.then(measure);
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(el);
    return () => watch.disconnect();
  }, [ready, size]);

  useLayoutEffect(() => {
    const frame = desk.current;
    const el = sheet.current;
    const holder = frame?.parentElement;
    if (!ready || !el || !holder) return;
    const measure = () => {
      const style = getComputedStyle(holder);
      const room =
        holder.clientWidth -
        Number.parseFloat(style.paddingLeft) -
        Number.parseFloat(style.paddingRight);
      const natural = el.offsetWidth;
      if (natural === 0 || room <= 0 || room >= natural) {
        setSqueeze(null);
        return;
      }
      const scale = room / natural;
      setSqueeze({ scale, width: natural * scale, height: el.offsetHeight * scale });
    };
    measure();
    void document.fonts?.ready.then(measure);
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(holder);
    watch.observe(el);
    return () => watch.disconnect();
  }, [ready, size]);

  useEffect(() => {
    document.title = `${t("screen.print.title")} · ${t("app.name")}`;
  }, []);

  if (page.isError) {
    const missing = page.error instanceof EngineError && page.error.code === "notFound";
    return (
      <main className={styles.message} aria-label={t("screen.print.title")}>
        <h1 className="sr-only">{t("screen.print.title")}</h1>
        <p role="alert">{missing ? t("print.missing") : t("print.error")}</p>
        {missing ? null : (
          <Button variant="secondary" onClick={() => void page.refetch()}>
            {t("button.tryAgain")}
          </Button>
        )}
        <BackLink to={back.to} label={back.label} />
      </main>
    );
  }
  if (!page.data) {
    return (
      <main className={styles.message} aria-label={t("screen.print.title")}>
        <h1 className="sr-only">{t("screen.print.title")}</h1>
        <p role="status">{t("print.loading")}</p>
      </main>
    );
  }

  const sentNothing = Boolean(privacy.data?.sentMeasured && privacy.data.sentToInternetBytes === 0);
  const choose = (next: PaperSize) => {
    setSize(next);
    rememberSize(next);
  };

  return (
    <main className={styles.screen} aria-label={t("screen.print.title")}>
      <h1 className="sr-only">{t("screen.print.title")}</h1>
      <style>{pageRule(size, receiptMm)}</style>
      <div className={styles.toolbar} role="toolbar" aria-label={t("print.toolbar")}>
        <BackLink to={back.to} label={back.label} />
        <fieldset className={styles.sizes}>
          <legend className={styles.sizesLabel}>{t("print.size.label")}</legend>
          {SIZES.map((s) => (
            <label key={s} className={styles.size}>
              <input type="radio" name="paper" checked={size === s} onChange={() => choose(s)} />
              <span>{t(`print.size.${s}`)}</span>
            </label>
          ))}
        </fieldset>
        <Button
          variant="primary"
          onClick={() => {
            platform.print().catch(() => toast.show({ message: t("print.failed") }));
          }}
        >
          {t("print.action")}
        </Button>
      </div>
      <div
        ref={desk}
        className={styles.desk}
        style={squeeze ? { width: squeeze.width, height: squeeze.height } : undefined}
      >
        <div
          className={styles.scaled}
          style={squeeze ? { transform: `scale(${squeeze.scale})` } : undefined}
        >
          <Sheet
            page={page.data}
            size={size}
            fitted={fit(page.data, size)}
            printedAt={printedAt}
            sentNothing={sentNothing}
            sheetRef={sheet}
          />
        </div>
      </div>
    </main>
  );
}
