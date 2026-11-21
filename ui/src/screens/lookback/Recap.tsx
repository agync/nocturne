import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { Icon } from "../../components/Icon/Icon";
import { PageLine } from "../../components/PageLine/PageLine";
import { richText } from "../../components/PageLine/richText";
import { receiptPath } from "../../components/ReceiptChip/receiptPath";
import { Thumb } from "../../components/Thumb/Thumb";
import { formatDayMonth } from "../../copy/format";
import { hasCopy, t } from "../../copy/t";
import type {
  CheckIn,
  DayCount,
  Highlight,
  ItemView,
  NameRef,
  Period,
  Recap,
  Together,
  WordCount,
} from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { FeelingMark } from "../../journal/FeelingMark";
import { usePlatform } from "../../platform/context";
import { CheckinTable, MonthChart, WeekChart, YearChart } from "./Charts";
import { HealthSection } from "./Health";
import styles from "./Lookback.module.css";
import { NameMark } from "./NameMark";
import { NoticedLines } from "./Noticed";
import { Chips, SpanText, useActiveReceipt } from "./Receipts";
import {
  clock,
  dayHref,
  dayParts,
  entityHref,
  itemHref,
  listWords,
  monthName,
  partName,
  weekdayDay,
} from "./text";

/** "9 to 15 November", "November 2026", "2025", with "So far, at 9:40 pm" while it goes on. */
export function periodText(period: Period): string {
  let text: string;
  if (period.kind === "week") {
    const start = dayParts(period.start);
    const end = dayParts(period.end);
    const first = start.month === end.month ? String(start.day) : `${start.day} ${start.month}`;
    text = t("journal.recap.eyebrow.week", { start: first, end: `${end.day} ${end.month}` });
  } else if (period.kind === "month") {
    text = t("journal.recap.eyebrow.month", {
      month: monthName(period.start),
      year: period.start.slice(0, 4),
    });
  } else {
    text = t("journal.recap.eyebrow.year", { year: period.start.slice(0, 4) });
  }
  return text;
}

export function recapEyebrow(recap: Recap): string {
  const text = periodText(recap.period);
  return recap.soFar
    ? `${text} · ${t("journal.recap.soFar", { time: clock(recap.writtenAt) })}`
    : text;
}

export const recapTitle = (kind: "week" | "month" | "year"): string =>
  t(`journal.recap.title.${kind}`);

function PhotoBox({ photo, size }: { photo: ItemView | null; size: "small" | "large" }) {
  const platform = usePlatform();
  const src = photo?.originalPath ? platform.originalSrc(photo.originalPath) : undefined;
  return (
    <span className={styles.photoBox} data-size={size} aria-hidden="true">
      <Thumb kind="image" size={size === "small" ? 20 : 28} src={src} />
    </span>
  );
}

function HighlightCard({ highlight: h }: { highlight: Highlight }) {
  const receipt = useActiveReceipt();
  const day = dayParts(h.day);
  const first = h.receipts[0];
  const lead = h.photo ? (
    <PhotoBox photo={h.photo} size="small" />
  ) : h.feeling ? (
    <span className={styles.feelChip} data-part={h.feeling.part}>
      <FeelingMark mark={h.feeling.part} size={14} />
      {h.feeling.words[0] ?? partName(h.feeling.part).toLowerCase()}
    </span>
  ) : (
    <Icon name="file-text" size={20} />
  );
  return (
    <li className={styles.hcard}>
      {lead}
      <div>
        {first ? (
          <a
            className={styles.hday}
            href={`#${receiptPath(first)}`}
            aria-label={t("journal.lookback.highlights.entry", {
              day: `${day.weekday} ${day.day}`,
            })}
          >
            {`${day.weekday} ${day.day}`}
          </a>
        ) : (
          <span className={styles.hday}>{`${day.weekday} ${day.day}`}</span>
        )}
        <span className={styles.hline}>
          {h.quoted ? (
            `“${h.text}”`
          ) : (
            <SpanText text={h.text} receipts={h.receipts} receipt={receipt} />
          )}
        </span>
      </div>
    </li>
  );
}

function Highlights({ recap }: { recap: Recap }) {
  const receipt = useActiveReceipt();
  if (recap.highlights.length === 0) return null;
  const receipts = recap.highlights.flatMap((h) => h.receipts);
  return (
    <section className={styles.section} aria-labelledby="sec-highlights">
      <h2 id="sec-highlights" className={styles.label}>
        {t("journal.recap.section.highlights")}
      </h2>
      {recap.period.kind === "year" ? (
        <ol className={styles.glance}>
          {recap.highlights.map((h) => (
            <li key={`${h.day}-${h.text}`} className={styles.monthRow}>
              <span className={styles.monthName}>{monthName(h.day)}</span>
              <span>
                {h.quoted ? (
                  `“${h.text}”`
                ) : (
                  <SpanText text={h.text} receipts={h.receipts} receipt={receipt} />
                )}
              </span>
            </li>
          ))}
        </ol>
      ) : (
        <ul className={styles.highlights}>
          {recap.highlights.map((h) => (
            <HighlightCard key={`${h.day}-${h.text}`} highlight={h} />
          ))}
        </ul>
      )}
      <Chips
        receipts={receipts}
        receipt={receipt}
        withDay={recap.period.kind === "year" ? "date" : "weekday"}
      />
    </section>
  );
}

function WordRow({
  word,
  max,
  checkins,
}: {
  word: WordCount;
  max: number;
  checkins: readonly CheckIn[];
}) {
  const [open, setOpen] = useState(false);
  const count = word.checkinIds.length;
  const mine = checkins.filter((c) => word.checkinIds.includes(c.id));
  return (
    <li data-part={word.part}>
      <button
        type="button"
        className={styles.wordName}
        aria-expanded={open}
        aria-label={`${word.word}, ${t("journal.lookback.uses", { count })}`}
        onClick={() => setOpen(!open)}
      >
        <FeelingMark mark={word.part} size={14} />
        {word.word}
      </button>
      <span
        className={styles.bar}
        style={{ width: `${(count / max) * 100}%` }}
        aria-hidden="true"
      />
      <span className={styles.barCount}>{count}</span>
      {open ? (
        <ul className={styles.wordUses}>
          {mine.map((c) => (
            <li key={c.id}>
              <a href={itemHref(c.id)}>{`${weekdayDay(c.day)} · ${clock(c.keptAt)}`}</a>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

function WordsTable({ words }: { words: readonly WordCount[] }) {
  return (
    <table className={styles.table}>
      <caption className="sr-only">{t("journal.recap.words")}</caption>
      <thead>
        <tr>
          <th scope="col">{t("journal.lookback.table.word")}</th>
          <th scope="col">{t("journal.lookback.table.part")}</th>
          <th scope="col">{t("journal.lookback.table.times")}</th>
        </tr>
      </thead>
      <tbody>
        {words.map((w) => (
          <tr key={w.word}>
            <td>{w.word}</td>
            <td>
              <span className={styles.partCell} data-part={w.part}>
                <FeelingMark mark={w.part} size={12} />
                {partName(w.part)}
              </span>
            </td>
            <td>{w.checkinIds.length}</td>
          </tr>
        ))}
      </tbody>
    </table>
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

function comparePeriod(recap: Recap): string {
  const previous = recap.compare?.previousPeriod;
  if (!previous) return "";
  return previous.kind === "year" ? previous.start.slice(0, 4) : monthName(previous.start);
}

function compareLine(recap: Recap): string | null {
  const c = recap.compare;
  if (!c || recap.period.kind === "week") return null;
  const period = comparePeriod(recap);
  const difference = Math.abs(c.count - c.previous);
  const id =
    c.count > c.previous
      ? "journal.recap.compare.more"
      : c.count < c.previous
        ? "journal.recap.compare.fewer"
        : "journal.recap.compare.same";
  return t(id, { count: c.count, difference, period });
}

/** "What tended to go together": the check-in lines, then what sleep, steps or workouts went with. */
function TogetherBlock({ recap }: { recap: Recap }) {
  const receipt = useActiveReceipt();
  const noticed = recap.health?.noticed ?? [];
  if (recap.together.length === 0 && noticed.length === 0) return null;
  return (
    <div className={styles.block}>
      <h3 className={styles.label}>{t("journal.lookback.togetherHeading")}</h3>
      {recap.together.map((item) => (
        <div
          key={`${item.with.entityId}-${item.part}-${item.word}`}
          className={styles.togetherBlock}
        >
          <p className={styles.together}>{togetherLine(item)}</p>
          <Chips receipts={item.receipts} receipt={receipt} withDay="weekday" />
        </div>
      ))}
      <NoticedLines items={noticed} period={recap.period.kind as "week" | "month" | "year"} />
      {noticed.length > 0 ? (
        <p className={styles.meta}>{t("journal.lookback.noticed.note")}</p>
      ) : null}
    </div>
  );
}

function Felt({ recap }: { recap: Recap }) {
  const kind = recap.period.kind;
  if (kind === "day") return null;
  const none = recap.checkins.length === 0;
  const top = recap.words.slice(0, 8);
  const max = Math.max(1, ...top.map((w) => w.checkinIds.length));
  const compare = compareLine(recap);
  return (
    <section className={styles.section} aria-labelledby="sec-felt">
      <h2 id="sec-felt" className={styles.label}>
        {t("journal.recap.section.felt")}
      </h2>
      {none ? (
        <>
          <p className={styles.prose}>{t(`journal.recap.noCheckins.${kind}`)}</p>
          <TogetherBlock recap={recap} />
        </>
      ) : (
        <>
          {kind === "week" ? <WeekChart period={recap.period} checkins={recap.checkins} /> : null}
          {kind === "month" ? <MonthChart period={recap.period} checkins={recap.checkins} /> : null}
          {kind === "year" ? <YearChart period={recap.period} checkins={recap.checkins} /> : null}
          <div className={styles.noPrint}>
            <Disclosure label={t("journal.recap.showTable")}>
              <CheckinTable checkins={recap.checkins} withDay />
            </Disclosure>
          </div>
          <div className={styles.twoCol}>
            <div className={styles.block}>
              <h3 className={styles.label}>{t("journal.recap.words")}</h3>
              <ul className={styles.counts}>
                {top.map((w) => (
                  <WordRow key={w.word} word={w} max={max} checkins={recap.checkins} />
                ))}
              </ul>
              <div className={styles.noPrint}>
                <Disclosure label={t("journal.recap.showTable")}>
                  <WordsTable words={recap.words} />
                </Disclosure>
              </div>
            </div>
            <div className={styles.block}>
              <TogetherBlock recap={recap} />
              {compare ? <p className={styles.meta}>{compare}</p> : null}
            </div>
          </div>
        </>
      )}
    </section>
  );
}

function Good({ recap }: { recap: Recap }) {
  if (recap.good.length === 0) return null;
  const year = recap.period.kind === "year";
  return (
    <section className={styles.section} aria-labelledby="sec-good">
      <h2 id="sec-good" className={styles.label}>
        {t("journal.recap.section.good")}
      </h2>
      <ul className={styles.goodGrid}>
        {recap.good.map((g) => (
          <li key={g.id}>
            <a className={styles.goodCard} href={itemHref(g.id)}>
              {g.photo ? (
                <PhotoBox photo={g.photo} size="large" />
              ) : (
                <span
                  className={styles.photoBox}
                  data-size="large"
                  data-sun="true"
                  aria-hidden="true"
                >
                  <FeelingMark mark="sun" size={20} />
                </span>
              )}
              <span>{g.text}</span>
              <span className={styles.hday}>{year ? monthName(g.day) : weekdayDay(g.day)}</span>
            </a>
          </li>
        ))}
      </ul>
    </section>
  );
}

function PersonRow({ entry }: { entry: DayCount }) {
  const [open, setOpen] = useState(false);
  const shown = open ? entry.days : entry.days.slice(0, 2);
  const more = entry.days.length - 2;
  return (
    <li className={styles.personRow}>
      <NameMark kind={entry.with.kind} />
      <span>
        <a href={entityHref(entry.with.entityId)}>
          {t("journal.recap.peopleDays", { name: entry.with.name, count: entry.days.length })}
        </a>
        <span className={styles.dayChips}>
          {shown.map((day) => (
            <a key={day} className={styles.dayChip} href={dayHref(day)}>
              {open || entry.days.length <= 2 ? formatDayMonth(day) : weekdayDay(day)}
            </a>
          ))}
          {!open && more > 0 ? (
            <button type="button" className={styles.dayChip} onClick={() => setOpen(true)}>
              {`+${more}`}
            </button>
          ) : null}
        </span>
      </span>
    </li>
  );
}

function FirstKept({ name }: { name: NameRef }) {
  const engine = useEngine();
  const first = name.itemIds[0];
  const item = useQuery({
    queryKey: ["item", first],
    queryFn: () => engine.getItem(first ?? ""),
    enabled: first !== undefined,
  });
  return (
    <li className={styles.firstRow}>
      <NameMark kind={name.kind} />
      <span>
        {item.data ? (
          <a href={entityHref(name.entityId)}>
            {t("journal.recap.firstKept", {
              name: name.name,
              date: formatDayMonth(item.data.keptAt),
            })}
          </a>
        ) : (
          <a href={entityHref(name.entityId)}>{name.name}</a>
        )}
      </span>
    </li>
  );
}

function PeopleAndNew({ recap }: { recap: Recap }) {
  const kind = recap.period.kind as "week" | "month" | "year";
  if (recap.people.length === 0 && recap.newNames.length === 0) return null;
  return (
    <section className={styles.section} aria-labelledby="sec-people">
      <div className={styles.twoCol}>
        <div className={styles.block}>
          <h2 id="sec-people" className={styles.label}>
            {t("journal.recap.section.people")}
          </h2>
          <ul className={styles.plainList}>
            {recap.people.map((entry) => (
              <PersonRow key={entry.with.entityId} entry={entry} />
            ))}
          </ul>
        </div>
        {recap.newNames.length > 0 ? (
          <div className={styles.block}>
            <h2 className={styles.label}>{t(`journal.recap.new.${kind}`)}</h2>
            {kind === "year" ? (
              <ul className={styles.plainList}>
                {recap.newNames.map((n) => (
                  <FirstKept key={n.entityId} name={n} />
                ))}
              </ul>
            ) : (
              <ul className={styles.chips}>
                {recap.newNames.map((n) => (
                  <li key={n.entityId}>
                    <a className={styles.nameChip} href={entityHref(n.entityId)}>
                      <NameMark kind={n.kind} />
                      {n.name}
                      <span className={styles.newTag}>{t("journal.people.new")}</span>
                    </a>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : null}
      </div>
    </section>
  );
}

function ideaText(idea: string): string {
  const id = `journal.help.idea.${idea}`;
  return hasCopy(id) ? t(id) : idea;
}

function whenWritten(recap: Recap): string {
  if (recap.written === "onDemand") {
    return t("journal.lookback.when.onDemand", { time: clock(recap.writtenAt) });
  }
  const night = new Date(recap.writtenAt);
  if (night.getHours() < 12) night.setDate(night.getDate() - 1);
  return t("journal.lookback.when.overnight", {
    weekday: new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(night),
  });
}

interface RecapViewProps {
  recap: Recap;
  onWriteAgain: () => void;
  onWriteWhenRead: () => void;
  waiting: boolean;
  onPrint: () => void;
  onDelete: () => void;
}

/** A week, month or year looked back on. */
export function RecapView({
  recap,
  onWriteAgain,
  onWriteWhenRead,
  waiting,
  onPrint,
  onDelete,
}: RecapViewProps) {
  const reading = recap.stillReading ?? 0;
  return (
    <>
      {reading > 0 ? (
        <div className={styles.notice}>
          <span>
            {t("journal.lookback.recap.partial", {
              count: reading,
              kind: recap.period.kind as "week" | "month" | "year",
            })}
          </span>
          <Button variant="secondary" disabled={waiting} onClick={onWriteWhenRead}>
            {t("journal.lookback.recap.partialAction", { count: reading })}
          </Button>
        </div>
      ) : null}
      {recap.linesRemoved > 0 ? (
        <div className={styles.notice}>
          <span>{t("journal.reflection.removed")}</span>
          <Button variant="secondary" onClick={onWriteAgain}>
            {t("journal.reflection.writeAgain")}
          </Button>
        </div>
      ) : null}
      <Highlights recap={recap} />
      <Felt recap={recap} />
      <HealthSection recap={recap} />
      <Good recap={recap} />
      <PeopleAndNew recap={recap} />

      {recap.tried.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-tried">
          <h2 id="sec-tried" className={styles.label}>
            {t("journal.recap.section.tried")}
          </h2>
          <ul className={styles.plainList}>
            {recap.tried.map((tried) => (
              <li key={`${tried.checkinId}-${tried.idea}`}>
                <a href={itemHref(tried.checkinId)}>
                  {`${weekdayDay(tried.day)} · ${ideaText(tried.idea)}`}
                </a>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {recap.changed.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-changed">
          <h2 id="sec-changed" className={styles.label}>
            {t("journal.recap.section.changed")}
          </h2>
          <div>
            {recap.changed.map((line) => (
              <PageLine
                key={line.id}
                line={line}
                onAction={(action) => {
                  if (action.type === "open") goTo(receiptPath(action.receipt));
                }}
              />
            ))}
          </div>
        </section>
      ) : null}

      {recap.period.kind !== "year" && recap.stillOpen.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-open">
          <h2 id="sec-open" className={styles.label}>
            {t("journal.recap.section.open")}
          </h2>
          <ul className={styles.openList}>
            {recap.stillOpen.map((m) => (
              <li key={m.id} className={styles.notice}>
                <span>
                  <strong>{m.subject}</strong>
                  <span className={styles.small}>{richText(m.sentence)}</span>
                </span>
                <a className={styles.quietLink} href="#/needs-you">
                  {t("journal.reflection.inNeedsYou")}
                </a>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <footer className={styles.footer}>
        <p className={styles.meta}>
          {t("journal.recap.footer", { when: whenWritten(recap), count: recap.days })}
        </p>
        <div className={`${styles.buttons} ${styles.noPrint}`}>
          {recap.soFar || recap.written === "onDemand" ? (
            <Button variant="secondary" onClick={onWriteAgain}>
              {t("journal.reflection.writeAgain")}
            </Button>
          ) : null}
          <Button variant="quiet" onClick={onPrint}>
            <Icon name="printer" size={16} />
            {t("edition.print")}
          </Button>
          <span className={styles.end}>
            <Button variant="quiet" onClick={onDelete}>
              {t("journal.lookback.recap.delete")}
            </Button>
          </span>
        </div>
      </footer>
    </>
  );
}
