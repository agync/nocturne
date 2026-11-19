import { useEffect, useRef, useState } from "react";
import { formatDuration, formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  EntryBlock,
  JournalDay,
  Mention,
  SameNameQuestion,
  TimelineRow,
} from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import { PARTS } from "../../journal/feelings";
import { addDays, dayWords, weekdayOf } from "../../screens/journal/time";
import {
  type Connection,
  ConnectionError,
  type PendingKeep,
  type PhoneEngine,
  PhoneRequestError,
  type Queue,
} from "../engine";
import { afterSheet, BottomSheet } from "./Dialog";
import { Button, Icon, Page } from "./ui";

/** How many days "Choose a day" lists, today first. */
const RECENT_DAYS = 14;

type Load =
  | { name: "loading" }
  | { name: "ready"; data: JournalDay; notReady: boolean }
  | { name: "asleep" }
  | { name: "failed" };

interface JournalProps {
  engine: PhoneEngine;
  queue: Queue;
  connection: Connection;
  computerName: string;
  /** The day shown, or null for today. */
  day: string | null;
  /** Today's journal day once known, so older days can step back to it and list recent ones. */
  today: string | null;
  onBack(): void;
  onDay(day: string, today: string | null): void;
  onOpen(itemId: string): void;
  onWhich(question: SameNameQuestion, day: string, today: string | null): void;
  onReflect(day: string, today: string | null): void;
  onReflection(day: string, today: string | null): void;
  onKeep(): void;
  onCheckIn(): void;
}

function entryParagraphs(blocks: readonly EntryBlock[]): string[] {
  return blocks
    .map((b) => (b.item.kind === "image" ? (b.alt ?? b.item.summary) : b.item.text))
    .filter((text): text is string => Boolean(text?.trim()));
}

/** The first two sentences of the entry, with an ellipsis when there is more. */
function firstLines(paragraphs: readonly string[]): { text: string; more: boolean } {
  const text = paragraphs.join(" ").trim();
  const sentences = text.match(/[^.!?]+[.!?]+(?:\s+|$)|[^.!?]+$/g) ?? [text];
  const lead = sentences.slice(0, 2).join("").trim();
  const more = lead.length < text.length || paragraphs.length > 1;
  return more ? { text: `${lead.replace(/[.!?]+$/, "")}…`, more } : { text: lead, more };
}

function Feel({
  part,
  words,
  named = false,
}: {
  part: keyof typeof PARTS;
  words: readonly string[];
  /** A check-in says its part before its words: "Low · tired". */
  named?: boolean;
}) {
  const name = t(PARTS[part].name);
  const text =
    words.length === 0
      ? name
      : named
        ? t("journal.checkin.feel", { part: name, words: words.join(", ") })
        : words.join(", ");
  return (
    <span className="ph-feelchip" data-part={part}>
      <FeelingMark mark={part} size={14} />
      {text}
    </span>
  );
}

function Names({ mentions }: { mentions: readonly Mention[] }) {
  const shown = mentions.filter((m) => m.state !== "removed");
  if (shown.length === 0) return null;
  return (
    <span className="ph-names">
      {shown.map((m) => (
        <span key={m.id} className="ph-name">
          {m.kind === "person" ? <Icon name="user" size={14} /> : null}
          {m.kind === "place" ? <Icon name="pin" size={14} /> : null}
          {m.name}
        </span>
      ))}
    </span>
  );
}

function lineOf(row: TimelineRow): string {
  switch (row.kind) {
    case "good":
      return row.good?.text ?? row.item.title;
    case "photo":
      return row.item.summary ?? row.item.title;
    case "voice":
      return row.item.text ?? t("kind.audio");
    case "elsewhere":
      return `${row.item.title} · ${t(`kind.${row.item.kind}`)}`;
    default:
      return row.item.text ?? row.item.title;
  }
}

function Row({ row, onOpen }: { row: TimelineRow; onOpen(id: string): void }) {
  const time = formatTime12(row.item.keptAt);
  const checkin = row.checkin;
  const quote = checkin?.line ? `“${checkin.line}”` : null;
  const body =
    row.kind === "checkin" ? (
      <>
        {checkin ? <Feel part={checkin.feeling.part} words={checkin.feeling.words} named /> : null}
        {quote ? <span>{quote}</span> : null}
        <Names mentions={row.mentions} />
      </>
    ) : row.kind === "good" ? (
      <span className="ph-goodline">
        <FeelingMark mark="sun" size={18} tone={false} />
        {lineOf(row)}
      </span>
    ) : row.kind === "photo" ? (
      <span className="ph-photoline">
        <span className="ph-thumb" aria-hidden="true">
          <Icon name="image" size={16} />
        </span>
        {lineOf(row)}
      </span>
    ) : row.kind === "elsewhere" ? (
      <span className="ph-dim">{lineOf(row)}</span>
    ) : (
      <>
        <span>
          {lineOf(row)}
          {row.feeling ? (
            <>
              {" "}
              <Feel part={row.feeling.feeling.part} words={row.feeling.feeling.words} />
            </>
          ) : null}
        </span>
        <Names mentions={row.mentions} />
        {row.item.fromDevice ? (
          <span className="ph-rc">
            {t("journal.timeline.fromDevice", { phone: row.item.fromDevice })}
          </span>
        ) : null}
      </>
    );
  return (
    <li>
      <button type="button" className="ph-trow" onClick={() => onOpen(row.item.id)}>
        <span className="ph-rc ph-tcol">
          {row.part === "afterMidnight" ? t("journal.timeline.afterMidnight", { time }) : time}
        </span>
        <span className="ph-tbody">{body}</span>
      </button>
    </li>
  );
}

/** What is waiting on this phone while the computer cannot be reached. */
function Waiting({ keeps }: { keeps: readonly PendingKeep[] }) {
  if (keeps.length === 0) return null;
  return (
    <section aria-labelledby="waiting-heading">
      <h2 id="waiting-heading" className="ph-lbl" style={{ marginBottom: 4 }}>
        {t("phone.journal.waiting.heading")}
      </h2>
      <ul className="ph-tl">
        {keeps.map((k) => {
          const text =
            k.kind === "text"
              ? k.text
              : k.kind === "photo"
                ? k.caption || t("kind.image")
                : k.kind === "recording"
                  ? `${t("kind.audio")}, ${formatDuration(Math.max(0, (k.size - 44) / 32))}`
                  : null;
          const feeling = k.checkIn?.feeling ?? k.feeling;
          return (
            <li key={k.clientId} className="ph-trow ph-trow--still">
              <span className="ph-rc ph-tcol">{formatTime12(new Date(k.createdAt))}</span>
              <span className="ph-tbody">
                {feeling ? <Feel part={feeling.part} words={feeling.words} /> : null}
                {text ? <span>{text}</span> : null}
                <span className="ph-waiting">{t("phone.journal.waiting.tag")}</span>
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function ChooseDay({
  today,
  shown,
  onPick,
  onClose,
}: {
  today: string;
  shown: string;
  onPick(day: string): void;
  onClose(): void;
}) {
  const days = Array.from({ length: RECENT_DAYS }, (_, i) => addDays(today, -i));
  return (
    <>
      <div className="ph-scrim" aria-hidden="true" onClick={onClose} />
      <BottomSheet label={t("journal.day.choose")} onClose={onClose}>
        <div className="ph-row-between">
          <h2>{t("journal.day.choose")}</h2>
          <button type="button" className="ph-ib ph-ib--accent" onClick={onClose}>
            {t("phone.done")}
          </button>
        </div>
        <ul className="ph-days">
          {days.map((d) => (
            <li key={d}>
              <button
                type="button"
                className="ph-dayrow"
                aria-current={d === shown ? "date" : undefined}
                onClick={() => onPick(d)}
              >
                <span>{t("journal.day.title", dayWords(d))}</span>
                {d === today ? <span className="ph-rc">{t("phone.journal.today")}</span> : null}
              </button>
            </li>
          ))}
        </ul>
      </BottomSheet>
    </>
  );
}

/** Today's journal on the phone, or an earlier day: the entry, the timeline, what needs you. */
export function Journal(p: JournalProps) {
  const { engine, queue, connection, day, today } = p;
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [waiting, setWaiting] = useState<PendingKeep[]>([]);
  const [open, setOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const entryText = useRef<HTMLDivElement>(null);
  const connected = connection.kind === "connected";

  useEffect(() => {
    void attempt;
    if (!connected) {
      setLoad({ name: "asleep" });
      return;
    }
    let gone = false;
    setLoad({ name: "loading" });
    (async () => {
      try {
        const data = await engine.getJournalDay(day ?? undefined);
        const status = await engine.status().then(
          (s) => s.readiness,
          () => "ready" as const,
        );
        if (!gone) setLoad({ name: "ready", data, notReady: status !== "ready" });
      } catch (e) {
        if (gone) return;
        if (e instanceof ConnectionError) return setLoad({ name: "asleep" });
        if (e instanceof PhoneRequestError || e instanceof Error)
          return setLoad({ name: "failed" });
        throw e;
      }
    })();
    return () => {
      gone = true;
    };
  }, [engine, day, connected, attempt]);

  const asleep = load.name === "asleep";
  useEffect(() => {
    if (!asleep) return;
    let gone = false;
    queue.list().then(
      (keeps) => !gone && setWaiting(keeps),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [asleep, queue]);

  const data = load.name === "ready" ? load.data : null;
  const shown = data?.day ?? day;
  const known = data?.today ? data.day : today;
  const rows = data?.rows.filter((r) => r.kind !== "entry") ?? [];
  const entry = data ? entryParagraphs(data.entry) : [];
  const lead = firstLines(entry);
  const isToday = data?.today ?? day === null;
  const dayName = shown ? t("journal.day.title", dayWords(shown)) : "";

  const goTo = (d: string) => p.onDay(d, known);

  const bar = (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={p.onBack}
        aria-label={t("phone.back")}
      >
        <Icon name="back" />
      </button>
      <h1 className="ph-bar-title">{t("journal.day.eyebrow")}</h1>
      {known && !asleep ? (
        <button type="button" className="ph-ib ph-ib--accent" onClick={() => setChoosing(true)}>
          {t("journal.day.choose")}
        </button>
      ) : null}
    </header>
  );

  const eyebrow = isToday ? t("phone.journal.eyebrow.today", dayWords(shown ?? "")) : dayName;

  if (load.name === "loading") {
    return (
      <Page bar={bar}>
        <main className="ph-main" aria-busy="true">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {t("phone.journal.loading")}
          </p>
        </main>
      </Page>
    );
  }

  if (load.name === "failed") {
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <h1 className="ph-h1 ph-h1--small" role="alert">
            {t("journal.timeline.error")}
          </h1>
          <div className="ph-spacer">
            <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
              {t("phone.tryAgain")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  if (load.name === "asleep") {
    return (
      <Page bar={bar}>
        <main className="ph-main ph-main--tight" style={{ gap: 18 }}>
          <p className="ph-lbl">{eyebrow}</p>
          <div className="ph-banner" role="status">
            <Icon name="moon" size={22} />
            <div>
              <p className="ph-banner-title">{t("phone.asleep.title", { name: p.computerName })}</p>
              <p className="ph-banner-body">{t("phone.journal.asleep")}</p>
            </div>
          </div>
          <Waiting keeps={waiting} />
        </main>
        <footer className="ph-foot">
          <Button kind="primary" onClick={p.onKeep}>
            {t("phone.off.keep")}
          </Button>
        </footer>
      </Page>
    );
  }

  const { data: day0 } = load;
  const reflection = day0.reflection;
  const empty = rows.length === 0 && entry.length === 0 && day0.checkins.length === 0;

  return (
    <Page bar={bar} className="ph-journal">
      <main className="ph-main ph-main--tight" style={{ gap: 14 }}>
        <p className="ph-lbl">{eyebrow}</p>
        {load.notReady ? (
          <div className="ph-card" role="status" style={{ gap: 4 }}>
            <p className="ph-card-title">{t("journal.notReady.title")}</p>
            <p className="ph-hint ph-hint--ink2">{t("journal.notReady.body")}</p>
          </div>
        ) : null}
        {day0.sameName.map((q) => (
          <button
            key={q.id}
            type="button"
            className="ph-card ph-card--row"
            onClick={() => p.onWhich(q, day0.day, known)}
          >
            <span className="ph-stack" style={{ gap: 6, alignItems: "flex-start" }}>
              <span className="ph-lbl">{t("phone.journal.needsYou")}</span>
              <span className="ph-card-title">{t("journal.people.which", { name: q.name })}</span>
              <span className="ph-badge">{t("journal.people.badge")}</span>
            </span>
            <Icon name="forward" size={20} />
          </button>
        ))}
        {entry.length > 0 ? (
          <section className="ph-card" aria-labelledby="entry-heading">
            <h2 id="entry-heading" className="ph-lbl" style={{ margin: 0 }}>
              {day0.today
                ? t("journal.entry.heading.today")
                : t("journal.entry.heading.day", { weekday: weekdayOf(day0.day) })}
            </h2>
            <div className="ph-quote" ref={entryText} tabIndex={-1}>
              {open ? (
                entry.map((paragraph) => <p key={paragraph}>{paragraph}</p>)
              ) : (
                <p>{lead.text}</p>
              )}
            </div>
            {!open && lead.more ? (
              <button
                type="button"
                className="ph-link"
                aria-expanded={open}
                onClick={() => {
                  setOpen(true);
                  setTimeout(() => entryText.current?.focus({ preventScroll: true }), 0);
                }}
              >
                {t("phone.journal.entry.readAll")}
              </button>
            ) : null}
          </section>
        ) : null}
        {reflection ? (
          <section className="ph-card" aria-labelledby="looked-heading">
            <h2 id="looked-heading" className="ph-lbl" style={{ margin: 0 }}>
              {t("journal.side.reflection")}
            </h2>
            <div className="ph-quote">
              <p>{reflection.firstSentence}</p>
            </div>
            <button
              type="button"
              className="ph-link"
              onClick={() => p.onReflection(day0.day, known)}
            >
              {t("journal.side.open")}
            </button>
          </section>
        ) : null}
        {rows.length > 0 ? (
          <section aria-labelledby="timeline-heading">
            <h2 id="timeline-heading" className="ph-lbl" style={{ marginBottom: 4 }}>
              {t("journal.timeline.heading")}
            </h2>
            <ul className="ph-tl">
              {rows.map((row) => (
                <Row key={`${row.kind}-${row.item.id}`} row={row} onOpen={p.onOpen} />
              ))}
            </ul>
          </section>
        ) : null}
        {empty ? (
          <div className="ph-stack" style={{ gap: 8, paddingTop: 8 }}>
            <p className="ph-serif ph-text">
              {day0.today ? t("phone.journal.empty.title") : t("phone.journal.empty.past")}
            </p>
            {day0.today ? <p className="ph-body">{t("phone.journal.empty.body")}</p> : null}
          </div>
        ) : null}
      </main>
      <footer className="ph-foot">
        {empty && day0.today ? (
          <>
            <Button kind="primary" onClick={p.onKeep}>
              {t("phone.off.keep")}
            </Button>
            <Button kind="quiet" onClick={p.onCheckIn}>
              {t("journal.side.checkIn")}
            </Button>
          </>
        ) : (
          <>
            {!reflection && !empty ? (
              <Button kind="primary" onClick={() => p.onReflect(day0.day, known)}>
                {day0.today ? t("journal.day.reflect") : t("journal.day.reflectPast")}
              </Button>
            ) : null}
            <Button kind="quiet" onClick={() => goTo(addDays(day0.day, -1))}>
              {t("journal.day.previous", { weekday: weekdayOf(addDays(day0.day, -1)) })}
            </Button>
            {!day0.today && known ? (
              <Button kind="quiet" onClick={() => goTo(known)}>
                {t("phone.journal.today")}
              </Button>
            ) : null}
          </>
        )}
      </footer>
      {choosing && known ? (
        <ChooseDay
          today={known}
          shown={day0.day}
          onClose={() => setChoosing(false)}
          onPick={(d) => afterSheet(() => goTo(d))}
        />
      ) : null}
    </Page>
  );
}
