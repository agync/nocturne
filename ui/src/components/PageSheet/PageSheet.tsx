import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { formatEta, formatLongDay, formatTime, formatWeekday } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  Edition,
  PageLine as Line,
  PageView,
  QuestionChoice,
  WhichResolved,
} from "../../engine/bindings";
import { Button } from "../Button/Button";
import { EditionTabs } from "../EditionTabs/EditionTabs";
import { type LineAction, PageLine } from "../PageLine/PageLine";
import { Progress } from "../Progress/Progress";
import { ReceiptChip } from "../ReceiptChip/ReceiptChip";
import { JournalQuestion } from "./JournalQuestion";
import styles from "./PageSheet.module.css";
import { CalendarToday, LastNight } from "./PhoneSections";
import { choiceText } from "./questionWords";

export type SheetAction =
  | LineAction
  | { type: "answer"; choice: QuestionChoice }
  | { type: "addLine"; line: string }
  | { type: "edition"; edition: Edition };

export interface WritingProgress {
  done: number;
  total: number;
  etaSeconds: number | null;
}

interface PageSheetProps {
  page: PageView;
  /** Only the screen edition is drawn here; Listen and Print are their own screens. */
  edition?: "screen";
  onAction: (action: SheetAction) => void;
  /** The day before this page, for the back link. */
  previousDate?: string | null;
  /** `HH:MM`, for the page-time question. */
  pageTime?: string;
  /** What has been sent to the internet, already worded: "nothing" or "84 MB". Absent when it was not measured. */
  sentToInternet?: string;
  /** How far the overnight tidy has got, while a page is still being written. */
  progress?: WritingProgress | null;
  /** False for a page from an earlier day: nothing is being written for it any more. */
  live?: boolean;
  /** What the engine said it settled for each which-is-right line, by line id. */
  resolved?: Record<string, WhichResolved>;
  /** Days a brought-back line stays clear of fading, from the letting-go pace. */
  fadeDays?: number;
  /** One quiet line under the page's last section, such as "Yesterday, looked back on ›". */
  after?: ReactNode;
}

function openCount(lines: Line[]): number {
  return lines.filter((l) => !l.done && !l.forgotten).length;
}

function Section({
  id,
  title,
  tone,
  children,
}: {
  id: string;
  title: string;
  tone?: "ember";
  children: React.ReactNode;
}) {
  return (
    <section className={styles.section} aria-labelledby={id}>
      <h2 id={id} className={`t-section ${styles.sectionTitle}`} data-tone={tone}>
        {title}
      </h2>
      {children}
    </section>
  );
}

const ANSWERS = {
  yes: ["page.question.answered.yes", "page.question.answered.yes.body"],
  no: ["page.question.answered.no", "page.question.answered.no.body"],
  notSure: ["page.question.answered.notSure", "page.question.answered.notSure.body"],
} as const;

/** Three bars that settle as the answer lands: all three for yes, two otherwise. */
function AnswerBars({ choice }: { choice: QuestionChoice }) {
  const [settled, setSettled] = useState(false);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setSettled(true));
    return () => cancelAnimationFrame(frame);
  }, []);
  const third = choice === "yes" && settled;
  return (
    <span className={styles.bars} aria-hidden="true">
      <span className={styles.bar} data-filled="true" />
      <span className={styles.bar} data-filled="true" />
      <span className={styles.bar} data-filled={third} />
    </span>
  );
}

export function PageSheet({
  page,
  onAction,
  previousDate = null,
  pageTime = "07:00",
  sentToInternet,
  progress = null,
  live = true,
  resolved = {},
  fadeDays = 30,
  after,
}: PageSheetProps) {
  const base = useId();
  const [changing, setChanging] = useState(false);
  const question = page.question;
  const isFirst = page.variant === "first";
  const heavy = page.variant === "heavy";
  const date = formatLongDay(page.date);

  const needsCount = openCount(page.needsYou);
  const changedShown = page.changed.length;

  const answered = question?.answered && !changing ? question.answered : null;
  const answerText = (choice: QuestionChoice): { title: string; body: string } => {
    if (choice === "right")
      return { title: t("page.question.answered.right", { time: pageTime }), body: "" };
    if (choice === "earlier") return { title: t("page.question.answered.earlier"), body: "" };
    if (choice === "later") return { title: t("page.question.answered.later"), body: "" };
    if (choice === "fine" || choice === "notGreat" || choice === "notYet")
      return { title: t("journal.question.answered", { choice: choiceText(choice) }), body: "" };
    const [title, body] = ANSWERS[choice];
    return { title: t(title), body: t(body) };
  };

  const change = useRef<HTMLButtonElement>(null);
  const asked = useRef(false);
  useEffect(() => {
    // The answer buttons are gone once answered, so focus moves to what replaced them.
    if (answered && asked.current) {
      asked.current = false;
      change.current?.focus();
    }
  }, [answered]);

  const choose = (choice: QuestionChoice) => {
    asked.current = true;
    setChanging(false);
    onAction({ type: "answer", choice });
  };

  return (
    <div className={styles.wrap} data-variant={page.variant}>
      <article className={styles.article} aria-label={t("page.article", { date })}>
        <div className={styles.top}>
          <EditionTabs
            current="screen"
            panelId={`${base}-panel`}
            tabIdFor={(e) => `${base}-tab-${e}`}
            onSelect={(edition) => onAction({ type: "edition", edition })}
          />
          <span className={styles.meta}>
            {t("page.number", { number: page.pageNumber })}
            {previousDate ? (
              <>
                {" · "}
                <a className={`${styles.previous} hit`} href={`#/pages/${previousDate}`}>
                  <span className="sr-only">{t("page.previous.hidden")}</span>{" "}
                  {t("page.previous", { day: formatWeekday(previousDate) })}
                </a>
              </>
            ) : null}
          </span>
        </div>

        <div
          role="tabpanel"
          id={`${base}-panel`}
          aria-labelledby={`${base}-tab-screen`}
          className={styles.panel}
        >
          <header className={styles.header}>
            <span className={`t-section ${styles.eyebrow}`}>
              {isFirst
                ? t("page.eyebrow.first", { date, number: page.pageNumber })
                : t("page.eyebrow", { date })}
            </span>
            <h1 className={styles.headline}>{page.headline}</h1>
            {page.lastNight ? (
              <LastNight
                id={`${base}-night`}
                night={page.lastNight}
                onOpen={(receipt) => onAction({ type: "open", receipt })}
              />
            ) : null}
            <p className={styles.dek}>
              {page.dek ||
                (page.variant === "catchUp"
                  ? t("page.catchUp.dek", { count: page.catchUpDays })
                  : "")}
            </p>
          </header>

          {page.variant === "beingWritten" && !live ? (
            <p className={styles.writingBody} role="note">
              {t("page.writing.past")}
            </p>
          ) : null}

          {page.variant === "beingWritten" && live ? (
            <div className={styles.writing}>
              <Progress
                value={progress?.done ?? 0}
                max={progress?.total ?? Math.max(1, page.pending)}
                label={t("page.writing.title")}
                detail={
                  progress
                    ? t("page.writing.detail.eta", {
                        done: progress.done,
                        total: progress.total,
                        eta: progress.etaSeconds !== null ? formatEta(progress.etaSeconds) : "",
                      }).replace(/ · $/, "")
                    : t("page.writing.detail.pending", { pending: page.pending })
                }
              />
              <p className={styles.writingBody}>{t("page.writing.body")}</p>
            </div>
          ) : null}

          {page.learned.length > 0 ? (
            <Section
              id={`${base}-learned`}
              title={t("page.section.learned", { count: page.learned.length })}
            >
              {page.learned.map((line) => (
                <PageLine key={line.id} line={line} onAction={onAction} fadeDays={fadeDays} />
              ))}
            </Section>
          ) : null}

          {isFirst ? (
            <section className={styles.next} aria-labelledby={`${base}-next`}>
              <h2 id={`${base}-next`} className={`t-section ${styles.sectionTitle}`}>
                {t("page.firstNext.title")}
              </h2>
              <p className={styles.nextBody}>{t("page.firstNext.body")}</p>
            </section>
          ) : null}

          {page.needsYou.length > 0 ? (
            <Section
              id={`${base}-needs`}
              tone="ember"
              title={
                heavy
                  ? t("page.section.needsYou.today")
                  : t("page.section.needsYou", { count: needsCount })
              }
            >
              {page.needsYou.map((line) => (
                <PageLine
                  key={line.id}
                  line={line}
                  onAction={onAction}
                  resolved={resolved[line.id]}
                />
              ))}
              {page.moreNeedsYou > 0 ? (
                <a className={styles.more} href="#/needs-you">
                  {t("page.more.needsYou", { count: page.moreNeedsYou })}
                </a>
              ) : null}
            </Section>
          ) : null}

          {page.calendarToday && page.calendarToday.length > 0 ? (
            <CalendarToday
              id={`${base}-calendar`}
              lines={page.calendarToday}
              more={page.moreCalendarToday ?? 0}
              day={page.date}
              onOpen={(receipt) => onAction({ type: "open", receipt })}
            />
          ) : null}

          {page.changed.length > 0 ? (
            <Section
              id={`${base}-changed`}
              title={
                page.moreChanged > 0
                  ? t("page.section.changed.top", {
                      shown: changedShown,
                      total: changedShown + page.moreChanged,
                    })
                  : t("page.section.changed", { count: changedShown })
              }
            >
              {page.changed.map((line) => (
                <PageLine key={line.id} line={line} onAction={onAction} fadeDays={fadeDays} />
              ))}
              {page.moreChanged > 0 ? (
                <a className={styles.more} href="#/memories">
                  {t("page.more.changed", { count: page.moreChanged })}
                </a>
              ) : null}
            </Section>
          ) : null}

          {page.upcoming.length > 0 ? (
            <Section id={`${base}-upcoming`} title={t("page.section.upcoming")}>
              {page.upcoming.map((line) => (
                <PageLine key={line.id} line={line} onAction={onAction} fadeDays={fadeDays} />
              ))}
            </Section>
          ) : null}

          {page.letGo.length > 0 ? (
            <Section
              id={`${base}-letgo`}
              title={t("page.section.letGo", { count: page.letGo.length })}
            >
              {page.letGo.map((line) => (
                <PageLine key={line.id} line={line} onAction={onAction} fadeDays={fadeDays} />
              ))}
            </Section>
          ) : null}

          {question?.kind === "journal" ? (
            <JournalQuestion
              id={base}
              question={question}
              onChoose={choose}
              onAddLine={(line) => onAction({ type: "addLine", line })}
              onOpenReceipt={(r) => onAction({ type: "open", receipt: r })}
            />
          ) : question ? (
            <section className={styles.question} aria-labelledby={`${base}-q`}>
              <h2 id={`${base}-q`} className={`t-section ${styles.sectionTitle}`}>
                {t("page.section.question")}
              </h2>
              <p className={styles.questionText}>
                {question.kind === "pageTime"
                  ? t("page.question.pageTime", { time: question.text })
                  : question.text}
              </p>
              {question.receipt ? (
                <span className={styles.questionSource}>
                  <ReceiptChip
                    receipt={question.receipt}
                    onOpen={(r) => onAction({ type: "open", receipt: r })}
                  />
                  <span className={styles.lastAsked}>
                    {question.lastAsked
                      ? t("page.question.lastAsked", { when: formatLongDay(question.lastAsked) })
                      : t("page.question.neverAsked")}
                  </span>
                </span>
              ) : null}
              {!answered ? (
                <div className={styles.answers}>
                  {question.kind === "yesNo" ? (
                    <>
                      <Button variant="primary" touch onClick={() => choose("yes")}>
                        {t("page.question.yes")}
                      </Button>
                      <Button variant="secondary" touch onClick={() => choose("no")}>
                        {t("page.question.no")}
                      </Button>
                      <Button variant="secondary" touch onClick={() => choose("notSure")}>
                        {t("page.question.notSure")}
                      </Button>
                    </>
                  ) : (
                    <>
                      <Button variant="primary" touch onClick={() => choose("right")}>
                        {t("page.question.right", { time: pageTime })}
                      </Button>
                      <Button variant="secondary" touch onClick={() => choose("earlier")}>
                        {t("page.question.earlier")}
                      </Button>
                      <Button variant="secondary" touch onClick={() => choose("later")}>
                        {t("page.question.later")}
                      </Button>
                    </>
                  )}
                </div>
              ) : null}
              <div className={styles.thanks} role="status" aria-live="polite">
                {answered ? (
                  <>
                    {question.kind === "yesNo" ? <AnswerBars choice={answered} /> : null}
                    <span className={styles.thanksTitle}>{answerText(answered).title}</span>
                    {answerText(answered).body ? (
                      <span className={styles.thanksBody}>{answerText(answered).body}</span>
                    ) : null}
                    <button
                      ref={change}
                      type="button"
                      className={styles.change}
                      onClick={() => {
                        asked.current = false;
                        setChanging(true);
                      }}
                    >
                      {t("button.changeAnswer")}
                    </button>
                  </>
                ) : null}
              </div>
            </section>
          ) : null}

          {after}

          {page.privateSkipped > 0 ? (
            <p className={styles.private}>
              {t("page.privateSkipped", { count: page.privateSkipped })}
            </p>
          ) : null}

          <footer className={styles.footer}>
            <span>
              {page.tidiedFrom && page.tidiedTo
                ? t("page.footer.tidied", {
                    from: formatTime(page.tidiedFrom),
                    to: formatTime(page.tidiedTo),
                  })
                : t("page.footer.notTidied")}
            </span>
            {sentToInternet === undefined ? (
              <span>
                {t("page.footer.unmeasured")}{" "}
                <a className={`${styles.more} hit`} href="#/privacy">
                  {t("page.footer.privacy")}
                </a>
              </span>
            ) : (
              <span>{t("page.footer.sent", { amount: sentToInternet })}</span>
            )}
          </footer>
        </div>
      </article>
    </div>
  );
}
