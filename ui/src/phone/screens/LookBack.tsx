import { Fragment, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { richText } from "../../components/PageLine/richText";
import { addDays, formatDuration, formatTime12, formatWeekday } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type {
  CheckIn,
  DayCount,
  DayReflection,
  Glance,
  GlanceKind,
  GlanceReason,
  Highlight,
  NameRef,
  Period,
  Recap,
  Receipt,
  ReflectionView,
  Together,
  WordCount,
} from "../../engine/bindings";
import { FeelingMap } from "../../journal/FeelingMap";
import { FeelingMark } from "../../journal/FeelingMark";
import { dayWords } from "../../screens/journal/time";
import { putTogetherId } from "../../screens/lookback/period";
import {
  checkinLabel,
  listWords,
  monthName,
  partName,
  receiptKey,
  weekdayDay,
  wordsOf,
} from "../../screens/lookback/text";
import { ConnectionError, type PhoneEngine } from "../engine";
import { BackBar, Button, Icon, Page } from "./ui";

/** How often the page asks again while the computer is still writing. */
const POLL_MS = 4000;

type Shown = ReflectionView | { type: "failed" };

interface LookBackProps {
  engine: PhoneEngine;
  period: Period;
  /** Asks the computer to write on arriving ("Reflect on today"). */
  write: boolean;
  /** Today's journal day, when known. */
  today: string | null;
  now(): number;
  /** The write has been asked for, so the address may drop its mark. */
  onStarted(): void;
  onBack(): void;
  onToday(): void;
  onItem(itemId: string): void;
  onReceipt(receipt: Receipt): void;
}

const KIND_COPY = {
  note: "journal.glance.kind.note",
  voice: "journal.glance.kind.voice",
  photo: "journal.glance.kind.photo",
  document: "journal.glance.kind.document",
  web: "journal.glance.kind.web",
} as const satisfies Record<GlanceKind, CopyId>;

function periodText(period: Period): string {
  if (period.kind === "week") {
    const start = dayWords(period.start);
    const end = dayWords(period.end);
    const first = start.month === end.month ? String(start.day) : `${start.day} ${start.month}`;
    return t("journal.recap.eyebrow.week", { start: first, end: `${end.day} ${end.month}` });
  }
  if (period.kind === "month") {
    return t("journal.recap.eyebrow.month", {
      month: monthName(period.start),
      year: period.start.slice(0, 4),
    });
  }
  return t("journal.recap.eyebrow.year", { year: period.start.slice(0, 4) });
}

/** What a receipt chip says: "5:10 pm · note", "Voice note · 0:12", "Mon 10 · entry". */
function chipText(receipt: Receipt, withDay: boolean): string {
  const known = receipt.span?.type === "time" || receipt.durationMs !== null;
  if (receipt.kind === "audio" && known) {
    const at = receipt.span?.type === "time" ? receipt.span.startMs : (receipt.durationMs ?? 0);
    return t("journal.lookback.chip.voice", { duration: formatDuration(at) });
  }
  const isEntry = /entry$/i.test(receipt.label);
  const kind = isEntry ? "entry" : t(`kind.${receipt.kind}.spoken` as CopyId);
  if (isEntry && !withDay) return receipt.label;
  return withDay
    ? t("journal.lookback.chip.day", { day: weekdayDay(receipt.keptAt), kind })
    : t("journal.lookback.chip.time", { time: formatTime12(receipt.keptAt), kind });
}

/** Text with `**spans**`; each span stands for the receipt in its place and opens the original. */
function Spans({
  text,
  receipts,
  onReceipt,
}: {
  text: string;
  receipts: readonly Receipt[];
  onReceipt(r: Receipt): void;
}) {
  const only = receipts.length === 1 ? receipts[0] : undefined;
  return (
    <>
      {text.split("**").map((part, index) => {
        const key = `${index}-${part}`;
        if (index % 2 === 0) return <Fragment key={key}>{part}</Fragment>;
        const source = receipts[(index - 1) / 2] ?? only;
        if (!source || part === "") return <strong key={key}>{part}</strong>;
        return (
          <button key={key} type="button" className="ph-span" onClick={() => onReceipt(source)}>
            {part}
          </button>
        );
      })}
    </>
  );
}

function Chips({
  receipts,
  onReceipt,
  withDay = false,
}: {
  receipts: readonly Receipt[];
  onReceipt(r: Receipt): void;
  withDay?: boolean;
}) {
  const seen = new Set<string>();
  const unique = receipts.filter((r) => {
    const key = receiptKey(r);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  if (unique.length === 0) return null;
  return (
    <ul className="ph-rchips" aria-label={t("journal.lookback.chips")}>
      {unique.map((r) => (
        <li key={receiptKey(r)}>
          <button type="button" className="ph-rchip" onClick={() => onReceipt(r)}>
            {chipText(r, withDay)}
          </button>
        </li>
      ))}
    </ul>
  );
}

function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section className="ph-rsection" aria-labelledby={id}>
      <h2 id={id} className="ph-lbl">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Names({ names }: { names: readonly NameRef[] }) {
  return (
    <ul className="ph-names">
      {names.map((n) => (
        <li key={n.entityId} className="ph-name">
          {n.kind === "person" ? <Icon name="user" size={14} /> : null}
          {n.kind === "place" ? <Icon name="pin" size={14} /> : null}
          {n.name}
        </li>
      ))}
    </ul>
  );
}

function FeltSentences({ felt, onItem }: { felt: readonly CheckIn[]; onItem(id: string): void }) {
  return (
    <p className="ph-serif ph-text">
      {felt.map((c) => {
        const text = c.line
          ? t("journal.lookback.felt.sentenceLine", {
              time: formatTime12(c.keptAt),
              words: wordsOf(c),
              line: c.line,
            })
          : t("journal.lookback.felt.sentence", {
              time: formatTime12(c.keptAt),
              words: wordsOf(c),
            });
        return (
          <Fragment key={c.id}>
            {text.split("**").map((part, index) => {
              const key = `${c.id}-${index}`;
              return index % 2 === 1 ? (
                <button key={key} type="button" className="ph-span" onClick={() => onItem(c.id)}>
                  {part}
                </button>
              ) : (
                <Fragment key={key}>{part}</Fragment>
              );
            })}{" "}
          </Fragment>
        );
      })}
    </p>
  );
}

function StillOpen({ items }: { items: Recap["stillOpen"] }) {
  if (items.length === 0) return null;
  return (
    <Section id="sec-open" title={t("journal.reflection.section.open")}>
      <ul className="ph-stack" style={{ gap: 10, margin: 0, padding: 0, listStyle: "none" }}>
        {items.map((m) => (
          <li key={m.id} className="ph-card" style={{ gap: 6 }}>
            <span className="ph-card-title">{m.subject}</span>
            <span className="ph-hint ph-hint--ink2">
              {richText(m.sentence)} {t("phone.journal.stillOpen")}
            </span>
          </li>
        ))}
      </ul>
    </Section>
  );
}

function Written({
  reflection: r,
  onItem,
  onReceipt,
}: {
  reflection: DayReflection;
  onItem(id: string): void;
  onReceipt(r: Receipt): void;
}) {
  const receipts = r.theDay.flatMap((line) => line.receipts);
  return (
    <>
      {r.stillReading > 0 ? (
        <p className="ph-hint ph-hint--ink2" role="status">
          {t("journal.reflection.partial", { count: r.stillReading })}
        </p>
      ) : null}
      {r.linesRemoved > 0 ? (
        <p className="ph-hint ph-hint--ink2">{t("journal.reflection.removed")}</p>
      ) : null}
      {r.theDay.length > 0 ? (
        <Section id="sec-day" title={t("journal.reflection.section.day")}>
          <p className="ph-serif ph-text">
            {r.theDay.map((line, index) => (
              <Fragment key={`${index}-${line.text}`}>
                <Spans text={line.text} receipts={line.receipts} onReceipt={onReceipt} />{" "}
              </Fragment>
            ))}
          </p>
          <Chips receipts={receipts} onReceipt={onReceipt} />
        </Section>
      ) : null}
      <Section id="sec-felt" title={t("journal.reflection.section.felt")}>
        {r.felt.length === 0 ? (
          <p className="ph-serif ph-text">{t("journal.reflection.feltEmpty")}</p>
        ) : (
          <>
            <FeelingMap
              checkins={r.felt}
              size={160}
              onOpen={(c) => onItem(c.id)}
              dotLabel={checkinLabel}
            />
            <FeltSentences felt={r.felt} onItem={onItem} />
          </>
        )}
      </Section>
      {r.good.length > 0 ? (
        <Section id="sec-good" title={t("journal.reflection.section.good")}>
          <ul className="ph-goods">
            {r.good.map((g) => (
              <li key={g.id}>
                <FeelingMark mark="sun" size={18} tone={false} />
                <button type="button" className="ph-plain" onClick={() => onItem(g.id)}>
                  {g.text}
                </button>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}
      {r.people.length > 0 ? (
        <Section id="sec-people" title={t("journal.reflection.section.people")}>
          <Names names={r.people} />
        </Section>
      ) : null}
      <StillOpen items={r.stillOpen} />
      <p className="ph-rc ph-footnote">
        {r.written === "overnight"
          ? t("phone.journal.footer.overnight", { count: r.fromCount })
          : t("phone.journal.footer.onDemand", {
              time: formatTime12(r.writtenAt),
              count: r.fromCount,
            })}
      </p>
    </>
  );
}

function togetherLine(item: Together): string {
  const what = item.word ?? (item.part ? partName(item.part).toLowerCase() : "");
  return t("journal.lookback.together", {
    count: item.count,
    total: item.total,
    name: item.with.name,
    what: listWords(what ? [what] : []),
  });
}

function whenWritten(recap: Recap): string {
  if (recap.written === "onDemand") {
    return t("journal.lookback.when.onDemand", { time: formatTime12(recap.writtenAt) });
  }
  const night = new Date(recap.writtenAt);
  if (night.getHours() < 12) night.setDate(night.getDate() - 1);
  return t("journal.lookback.when.overnight", {
    weekday: new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(night),
  });
}

function HighlightCard({
  highlight: h,
  onReceipt,
}: {
  highlight: Highlight;
  onReceipt(r: Receipt): void;
}) {
  const day = dayWords(h.day);
  return (
    <li className="ph-hl">
      <span className="ph-rc">{`${day.weekday} ${day.day}`}</span>
      <span className="ph-serif ph-hline">
        {h.quoted ? (
          `“${h.text}”`
        ) : (
          <Spans text={h.text} receipts={h.receipts} onReceipt={onReceipt} />
        )}
      </span>
    </li>
  );
}

function WordBars({ words }: { words: readonly WordCount[] }) {
  const top = words.slice(0, 8);
  const max = Math.max(1, ...top.map((w) => w.checkinIds.length));
  return (
    <ul className="ph-counts">
      {top.map((w) => (
        <li key={w.word} data-part={w.part}>
          <span className="ph-count-name">
            <FeelingMark mark={w.part} size={14} />
            {w.word}
          </span>
          <span
            className="ph-bar-fill"
            style={{ width: `${(w.checkinIds.length / max) * 100}%` }}
            aria-hidden="true"
          />
          <span className="ph-rc">{w.checkinIds.length}</span>
        </li>
      ))}
    </ul>
  );
}

function CheckinTable({ checkins }: { checkins: readonly CheckIn[] }) {
  return (
    <table className="ph-table">
      <caption className="sr-only">{t("journal.lookback.table.checkins")}</caption>
      <thead>
        <tr>
          <th scope="col">{t("journal.lookback.table.day")}</th>
          <th scope="col">{t("journal.lookback.table.time")}</th>
          <th scope="col">{t("journal.lookback.table.part")}</th>
          <th scope="col">{t("journal.lookback.table.words")}</th>
        </tr>
      </thead>
      <tbody>
        {checkins.map((c) => (
          <tr key={c.id}>
            <td>{weekdayDay(c.day)}</td>
            <td>{formatTime12(c.keptAt)}</td>
            <td>{partName(c.feeling.part)}</td>
            <td>{wordsOf(c)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Seven small fields for a week, one for a longer period. */
function FeltChart({ recap }: { recap: Recap }) {
  const { period, checkins } = recap;
  if (period.kind !== "week") {
    return <FeelingMap checkins={checkins} size={220} corners />;
  }
  return (
    <ul className="ph-week" aria-label={t("journal.lookback.chart.week")}>
      {Array.from({ length: 7 }, (_, i) => addDays(period.start, i)).map((d) => (
        <li key={d}>
          <FeelingMap checkins={checkins.filter((c) => c.day === d)} size={46} corners={false} />
          <span className="ph-rc" aria-hidden="true">
            {formatWeekday(d).slice(0, 1)}
          </span>
          <span className="sr-only">{weekdayDay(d)}</span>
        </li>
      ))}
    </ul>
  );
}

function RecapBody({ recap, onReceipt }: { recap: Recap; onReceipt(r: Receipt): void }) {
  const [table, setTable] = useState(false);
  const kind = recap.period.kind as "week" | "month" | "year";
  return (
    <>
      {recap.stillReading ? (
        <p className="ph-hint ph-hint--ink2" role="status">
          {t("journal.lookback.recap.partial", { count: recap.stillReading, kind })}
        </p>
      ) : null}
      {recap.highlights.length > 0 ? (
        <Section id="sec-highlights" title={t("journal.recap.section.highlights")}>
          <ul className="ph-hls">
            {recap.highlights.map((h) => (
              <HighlightCard key={`${h.day}-${h.text}`} highlight={h} onReceipt={onReceipt} />
            ))}
          </ul>
        </Section>
      ) : null}
      <Section id="sec-felt" title={t("journal.recap.section.felt")}>
        {recap.checkins.length === 0 ? (
          <p className="ph-serif ph-text">{t(`journal.recap.noCheckins.${kind}`)}</p>
        ) : (
          <>
            <FeltChart recap={recap} />
            <h3 className="ph-lbl">{t("journal.recap.words")}</h3>
            <WordBars words={recap.words} />
            <button
              type="button"
              className="ph-link"
              aria-expanded={table}
              onClick={() => setTable(!table)}
            >
              {t("journal.recap.showTable")}
            </button>
            {table ? <CheckinTable checkins={recap.checkins} /> : null}
            {recap.together.length > 0 ? (
              <>
                <h3 className="ph-lbl">{t("journal.lookback.togetherHeading")}</h3>
                {recap.together.map((item) => (
                  <div
                    key={`${item.with.entityId}-${item.part}-${item.word}`}
                    className="ph-together"
                  >
                    <p>{togetherLine(item)}</p>
                    <Chips receipts={item.receipts} onReceipt={onReceipt} withDay />
                  </div>
                ))}
              </>
            ) : null}
          </>
        )}
      </Section>
      {recap.good.length > 0 ? (
        <Section id="sec-good" title={t("journal.recap.section.good")}>
          <ul className="ph-goodgrid">
            {recap.good.map((g) => (
              <li key={g.id} className="ph-goodcard">
                <span className="ph-goodicon" aria-hidden="true">
                  <FeelingMark mark="sun" size={20} tone={false} />
                </span>
                <span>{g.text}</span>
                <span className="ph-rc">
                  {kind === "year" ? monthName(g.day) : weekdayDay(g.day)}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}
      {recap.people.length > 0 ? (
        <Section id="sec-people" title={t("journal.recap.section.people")}>
          <ul className="ph-plainlist">
            {recap.people.map((entry: DayCount) => (
              <li key={entry.with.entityId}>
                {entry.with.kind === "person" ? <Icon name="user" size={16} /> : null}
                {entry.with.kind === "place" ? <Icon name="pin" size={16} /> : null}
                <span>
                  {t("journal.recap.peopleDays", {
                    name: entry.with.name,
                    count: entry.days.length,
                  })}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}
      {kind !== "year" ? <StillOpen items={recap.stillOpen} /> : null}
      <p className="ph-rc ph-footnote">
        {t("phone.journal.recap.footer", { when: whenWritten(recap), count: recap.days })}
      </p>
    </>
  );
}

function GlanceBody({ glance }: { glance: Glance }) {
  const total = glance.kept.reduce((n, k) => n + k.itemIds.length, 0);
  const people = glance.with.filter((n) => n.kind === "person");
  const places = glance.with.filter((n) => n.kind !== "person");
  return (
    <Section id="glance-heading" title={t("journal.glance.heading")}>
      <ul className="ph-glance">
        {glance.kept.length > 0 ? (
          <li>
            {t("journal.glance.kept", {
              count: total,
              kinds: listWords(
                glance.kept.map((k) => t(KIND_COPY[k.kind], { count: k.itemIds.length })),
              ),
            })}
          </li>
        ) : null}
        {glance.felt.length > 0 ? (
          <li>
            {t("journal.glance.felt", {
              items: glance.felt
                .map((c) =>
                  t("journal.glance.feltItem", {
                    words: wordsOf(c),
                    time: formatTime12(c.keptAt),
                  }),
                )
                .join(", "),
            })}
          </li>
        ) : null}
        {glance.good.map((g) => (
          <li key={g.id} className="ph-goodline">
            <FeelingMark mark="sun" size={16} tone={false} />
            {g.text}
          </li>
        ))}
        {people.length > 0 ? (
          <li>
            {t("journal.glance.with", {
              names: people
                .map((n) => (n.new ? t("journal.glance.newName", { name: n.name }) : n.name))
                .join(", "),
            })}
          </li>
        ) : null}
        {places.length > 0 ? (
          <li>{t("journal.glance.at", { names: places.map((n) => n.name).join(", ") })}</li>
        ) : null}
        {glance.entryLines.length > 0 ? <li>{`“${glance.entryLines.join(" ")}”`}</li> : null}
      </ul>
    </Section>
  );
}

function glanceLead(reason: GlanceReason): string | null {
  if (reason === "notReady") return t("journal.reflection.glance.notReady");
  if (reason === "notOnThisComputer") return t("journal.reflection.glance.notOnThisComputer");
  return null;
}

function Writing() {
  return (
    <div className="ph-stack" style={{ gap: 10 }} role="status">
      <p className="ph-writing">
        <span className="ph-pulse" aria-hidden="true" />
        {t("phone.journal.writing.title")}
      </p>
      <p className="ph-body">{t("phone.journal.writing.body")}</p>
    </div>
  );
}

/** A day's reflection or a week, month or year looked back on, in one column. */
export function LookBack(p: LookBackProps) {
  const { engine } = p;
  const { kind, start, end } = p.period;
  const period = useMemo<Period>(() => ({ kind, start, end }), [kind, start, end]);
  const [view, setView] = useState<Shown | null>(null);
  const [writing, setWriting] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const writeNext = useRef(p.write);
  const lastCall = useRef<"read" | "write">("read");
  const [failedCall, setFailedCall] = useState<"read" | "write">("read");
  const onStarted = useRef(p.onStarted);
  onStarted.current = p.onStarted;

  // Reads what is there; when told to, asks the computer to write first.
  useEffect(() => {
    void attempt;
    let gone = false;
    const settle = (next: ReflectionView) => {
      if (!gone) setView(next);
    };
    const failed = (e: unknown) => {
      if (gone) return;
      if (e instanceof Error) {
        setFailedCall(lastCall.current);
        setView({ type: "failed" });
      } else throw e;
    };
    if (writeNext.current) {
      writeNext.current = false;
      lastCall.current = "write";
      setWriting(true);
      onStarted.current();
      engine.lookBack(period).then(
        (next) => {
          if (gone) return;
          setWriting(false);
          settle(next);
        },
        (e) => {
          if (!gone) setWriting(false);
          failed(e);
        },
      );
    } else {
      lastCall.current = "read";
      engine.getLookBack(period).then(settle, failed);
    }
    return () => {
      gone = true;
    };
  }, [engine, period, attempt]);

  // While the computer is still writing, ask again every few seconds; stop on leaving.
  const engineWriting = view?.type === "writing";
  useEffect(() => {
    if (!engineWriting) return;
    let gone = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = () => {
      if (gone) return;
      if (document.visibilityState === "hidden") {
        timer = setTimeout(poll, POLL_MS);
        return;
      }
      engine.getLookBack(period).then(
        (next) => {
          if (gone) return;
          setView(next);
          if (next.type === "writing") timer = setTimeout(poll, POLL_MS);
        },
        (e: unknown) => {
          if (gone) return;
          if (e instanceof ConnectionError) {
            timer = setTimeout(poll, POLL_MS);
            return;
          }
          lastCall.current = "read";
          setFailedCall("read");
          setView({ type: "failed" });
        },
      );
    };
    timer = setTimeout(poll, POLL_MS);
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [engineWriting, engine, period]);

  const showing = view?.type;
  useEffect(() => {
    if (showing) heading.current?.focus({ preventScroll: true });
  }, [showing]);

  const isDay = period.kind === "day";
  const isToday = isDay && p.today === period.start;
  const over = p.today !== null && period.end < p.today;
  const written = view?.type === "written" ? view.reflection : null;
  const recap = view?.type === "recap" ? view.recap : null;
  const day = isDay ? dayWords(period.start) : null;
  const title = isDay
    ? t("journal.day.title", day ?? {})
    : t(`journal.recap.title.${period.kind as "week" | "month" | "year"}`);
  let eyebrow: string;
  if (written?.soFar) {
    eyebrow = t("journal.reflection.eyebrow.soFar", { time: formatTime12(written.writtenAt) });
  } else if (isDay) {
    eyebrow = isToday
      ? t("journal.reflection.eyebrow.soFar", { time: formatTime12(new Date(p.now())) })
      : t("journal.reflection.eyebrow.finished", day ?? {});
  } else {
    eyebrow = periodText(period);
    if (recap?.soFar) {
      eyebrow = `${eyebrow} · ${t("journal.recap.soFar", { time: formatTime12(recap.writtenAt) })}`;
    }
  }

  const ask = () => {
    writeNext.current = true;
    setAttempt((n) => n + 1);
  };

  const retry = () => {
    writeNext.current = failedCall === "write";
    setAttempt((n) => n + 1);
  };

  let body: ReactNode = null;
  if (writing || view?.type === "writing") {
    body = <Writing />;
  } else if (view === null) {
    body = (
      <p className="ph-status" role="status" style={{ fontSize: "1rem" }} aria-busy="true">
        <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
        {t("phone.journal.loading")}
      </p>
    );
  } else if (view.type === "written") {
    body = <Written reflection={view.reflection} onItem={p.onItem} onReceipt={p.onReceipt} />;
  } else if (view.type === "recap") {
    body = <RecapBody recap={view.recap} onReceipt={p.onReceipt} />;
  } else if (view.type === "glance") {
    const lead = glanceLead(view.reason);
    body = (
      <>
        {lead ? <p className="ph-body">{lead}</p> : null}
        <GlanceBody glance={view.glance} />
      </>
    );
  } else if (view.type === "notYet") {
    body = (
      <>
        <GlanceBody glance={view.glance} />
        <Button kind="primary" onClick={ask}>
          {isDay
            ? t(isToday ? "journal.day.reflect" : "journal.day.reflectPast")
            : t(putTogetherId(period.kind as "week" | "month" | "year", over))}
        </Button>
      </>
    );
  } else if (view.type === "nothing") {
    body = (
      <p className="ph-serif ph-text">
        {isDay
          ? t("journal.reflection.nothing")
          : t(`journal.recap.nothing.${period.kind as "week" | "month" | "year"}`)}
      </p>
    );
  } else {
    body = (
      <div className="ph-stack" style={{ gap: 12 }} role="alert">
        <p className="ph-body">
          {failedCall === "write" ? t("journal.reflection.failed") : t("journal.timeline.error")}
        </p>
        <Button kind="quiet" onClick={retry}>
          {t("phone.tryAgain")}
        </Button>
      </div>
    );
  }

  return (
    <Page bar={<BackBar title={t("journal.day.eyebrow")} onBack={p.onBack} />}>
      <main className="ph-main ph-main--tight ph-look" style={{ gap: 16 }}>
        <header className="ph-stack" style={{ gap: 6 }}>
          <p className="ph-lbl">{eyebrow}</p>
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {title}
          </h1>
        </header>
        {body}
      </main>
      {isDay && view !== null ? (
        <footer className="ph-foot">
          <Button kind="quiet" onClick={p.onToday}>
            {t("phone.journal.today")}
          </Button>
        </footer>
      ) : null}
    </Page>
  );
}
