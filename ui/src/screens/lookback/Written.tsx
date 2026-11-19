import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { Icon } from "../../components/Icon/Icon";
import { richText } from "../../components/PageLine/richText";
import { t } from "../../copy/t";
import type { CheckIn, DayReflection, NameRef } from "../../engine/bindings";
import { FeelingMap } from "../../journal/FeelingMap";
import { FeelingMark } from "../../journal/FeelingMark";
import { CheckinTable } from "./Charts";
import styles from "./Lookback.module.css";
import { NameMark } from "./NameMark";
import { Chips, SpanText, useActiveReceipt } from "./Receipts";
import { checkinLabel, clock, dayParts, entityHref, itemHref, wordsOf } from "./text";

export function dayEyebrow(reflection: DayReflection): string {
  if (reflection.soFar)
    return t("journal.reflection.eyebrow.soFar", { time: clock(reflection.writtenAt) });
  return t("journal.reflection.eyebrow.finished", dayParts(reflection.period.start));
}

export function dayTitle(day: string): string {
  return t("journal.day.title", dayParts(day));
}

/** One check-in as a sentence; its words are bold and open the check-in. */
function FeltSentence({ checkin: c }: { checkin: CheckIn }) {
  const text = c.line
    ? t("journal.lookback.felt.sentenceLine", {
        time: clock(c.keptAt),
        words: wordsOf(c),
        line: c.line,
      })
    : t("journal.lookback.felt.sentence", { time: clock(c.keptAt), words: wordsOf(c) });
  return (
    <>
      {text.split("**").map((part, index) => {
        const key = `${index}-${part}`;
        return index % 2 === 1 ? (
          <a key={key} className={styles.span} href={itemHref(c.id)}>
            {part}
          </a>
        ) : (
          part
        );
      })}{" "}
    </>
  );
}

function FeltSection({ felt }: { felt: readonly CheckIn[] }) {
  if (felt.length === 0) {
    return <p className={styles.prose}>{t("journal.reflection.feltEmpty")}</p>;
  }
  return (
    <div className={styles.feltLines}>
      <FeelingMap
        checkins={felt}
        size={200}
        onOpen={(c) => goTo(`/items/${c.id}`)}
        dotLabel={checkinLabel}
      />
      <div className={styles.feltText}>
        <p className={styles.prose}>
          {felt.map((c) => (
            <FeltSentence key={c.id} checkin={c} />
          ))}
        </p>
        <div className={styles.noPrint}>
          <Disclosure label={t("journal.reflection.showTable")}>
            <CheckinTable checkins={felt} />
          </Disclosure>
        </div>
      </div>
    </div>
  );
}

function PeopleChips({ people }: { people: readonly NameRef[] }) {
  return (
    <ul className={styles.chips}>
      {people.map((n) => (
        <li key={n.entityId}>
          <a className={styles.nameChip} href={entityHref(n.entityId)}>
            <NameMark kind={n.kind} />
            {n.name}
            {n.new ? <span className={styles.newTag}>{t("journal.people.new")}</span> : null}
          </a>
        </li>
      ))}
    </ul>
  );
}

interface WrittenProps {
  reflection: DayReflection;
  onWriteAgain: () => void;
  onPrint: () => void;
  onDelete: () => void;
}

/** The day's reflection: what happened, how it felt, what was good, who and where, what is open. */
export function WrittenView({ reflection: r, onWriteAgain, onPrint, onDelete }: WrittenProps) {
  const receipt = useActiveReceipt();
  const paragraph = r.theDay.flatMap((line) => line.receipts);
  return (
    <>
      {r.stillReading > 0 ? (
        <div className={styles.notice}>
          <span>{t("journal.reflection.partial", { count: r.stillReading })}</span>
          <Button variant="secondary" onClick={onWriteAgain}>
            {t("journal.reflection.partialAction", { count: r.stillReading })}
          </Button>
        </div>
      ) : null}
      {r.linesRemoved > 0 ? (
        <div className={styles.notice}>
          <span>{t("journal.reflection.removed")}</span>
          <Button variant="secondary" onClick={onWriteAgain}>
            {t("journal.reflection.writeAgain")}
          </Button>
        </div>
      ) : null}

      {r.theDay.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-day">
          <h2 id="sec-day" className={styles.label}>
            {t("journal.reflection.section.day")}
          </h2>
          <p className={styles.rtext}>
            {r.theDay.map((line, index) => (
              <span key={`${index}-${line.text}`}>
                <SpanText text={line.text} receipts={line.receipts} receipt={receipt} />{" "}
              </span>
            ))}
          </p>
          <Chips receipts={paragraph} receipt={receipt} />
        </section>
      ) : null}

      <section className={styles.section} aria-labelledby="sec-felt">
        <h2 id="sec-felt" className={styles.label}>
          {t("journal.reflection.section.felt")}
        </h2>
        <FeltSection felt={r.felt} />
      </section>

      {r.good.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-good">
          <h2 id="sec-good" className={styles.label}>
            {t("journal.reflection.section.good")}
          </h2>
          <ul className={styles.goodList}>
            {r.good.map((g) => (
              <li key={g.id}>
                <FeelingMark mark="sun" size={16} />
                <a href={itemHref(g.id)}>{g.text}</a>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {r.people.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-people">
          <h2 id="sec-people" className={styles.label}>
            {t("journal.reflection.section.people")}
          </h2>
          <PeopleChips people={r.people} />
        </section>
      ) : null}

      {r.stillOpen.length > 0 ? (
        <section className={styles.section} aria-labelledby="sec-open">
          <h2 id="sec-open" className={styles.label}>
            {t("journal.reflection.section.open")}
          </h2>
          <ul className={styles.openList}>
            {r.stillOpen.map((m) => (
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
          {r.written === "overnight"
            ? t("journal.reflection.footer.overnight", { count: r.fromCount })
            : t("journal.reflection.footer.onDemand", {
                time: clock(r.writtenAt),
                count: r.fromCount,
              })}
        </p>
        <div className={`${styles.buttons} ${styles.noPrint}`}>
          <Button variant="secondary" onClick={onWriteAgain}>
            {t("journal.reflection.writeAgain")}
          </Button>
          <Button variant="quiet" onClick={onPrint}>
            <Icon name="printer" size={16} />
            {t("edition.print")}
          </Button>
          <span className={styles.end}>
            <Button variant="quiet" onClick={onDelete}>
              {t("journal.reflection.delete")}
            </Button>
          </span>
        </div>
      </footer>
    </>
  );
}
