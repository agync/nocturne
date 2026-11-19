import { useState } from "react";
import { useLocation } from "wouter";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { formatDay, formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type { CheckIn, ItemView, Mention } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { FeelingMap } from "../../journal/FeelingMap";
import { FeelingMark } from "../../journal/FeelingMark";
import { PARTS } from "../../journal/feelings";
import { MentionCard } from "../../journal/MentionCard";
import { MentionChip } from "../../journal/MentionChip";
import own from "./Checkin.module.css";
import { DeleteCheckin } from "./DeleteCheckin";
import styles from "./Item.module.css";

/** "Low · tired", or just "Low" when no words were chosen. */
const feelOf = (checkin: CheckIn): string => {
  const part = t(PARTS[checkin.feeling.part].name);
  return checkin.feeling.words.length > 0
    ? t("journal.checkin.feel", { part, words: checkin.feeling.words.join(", ") })
    : part;
};

/**
 * A check-in's own page: the field with its dot and the line beside what it was, never the
 * memory actions, because a check-in yields names and nothing else.
 */
export function CheckinPage({ item, checkin }: { item: ItemView; checkin: CheckIn }) {
  const engine = useEngine();
  const refresh = useRefresh();
  const toast = useToast();
  const [, navigate] = useLocation();
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [card, setCard] = useState<{ mention: Mention; anchor: HTMLElement } | null>(null);
  const time = formatTime12(checkin.keptAt);
  const feel = feelOf(checkin);
  const mentions = checkin.mentions.filter((m) => m.state !== "removed");

  const toggleLeftOut = async () => {
    setBusy(true);
    try {
      await engine.setLeftOut(item.id, !checkin.leftOut);
      refresh("item", "journal");
      toast.show({
        message: t(checkin.leftOut ? "item.checkin.backIn" : "journal.timeline.leftOut"),
      });
    } catch {
      toast.show({ message: t("item.failed") });
    } finally {
      setBusy(false);
    }
  };

  return (
    <ScreenSheet flush>
      <div className={styles.card}>
        <div className={own.left}>
          <div role="img" aria-label={t("item.checkin.field", { feel, time })}>
            <FeelingMap checkins={[checkin]} size={240} />
          </div>
          {checkin.line ? <p className={own.quote}>{`“${checkin.line}”`}</p> : null}
        </div>
        <div className={styles.details}>
          <BackLink to={`/journal/${checkin.day}`} label={t("screen.journal.title")} />
          <p className={`t-section ${styles.meta}`}>
            {t("item.checkin.eyebrow", { day: formatDay(checkin.keptAt), time })}
          </p>
          <h1 className={`t-item-title ${own.title}`} tabIndex={-1}>
            <FeelingMark mark={checkin.feeling.part} size={28} />
            {feel}
          </h1>
          {checkin.line ? (
            <section className={own.section} aria-labelledby="ci-line">
              <h2 id="ci-line" className="t-section">
                {t("item.checkin.line")}
              </h2>
              <p>{checkin.line}</p>
            </section>
          ) : null}
          {checkin.urge ? (
            <section className={own.section} aria-labelledby="ci-urge">
              <h2 id="ci-urge" className="t-section">
                {t("item.checkin.urge")}
              </h2>
              <p>{checkin.urge}</p>
            </section>
          ) : null}
          {mentions.length > 0 ? (
            <section className={own.section} aria-labelledby="ci-names">
              <h2 id="ci-names" className="t-section">
                {t("item.checkin.names")}
              </h2>
              <ul className={own.names}>
                {mentions.map((m) => (
                  <li key={m.id}>
                    <MentionChip
                      name={m.name}
                      kind={m.kind}
                      isNew={m.new}
                      pressed={card?.mention.id === m.id}
                      onClick={(event) => setCard({ mention: m, anchor: event.currentTarget })}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          <p className={own.note}>{t("item.checkin.note")}</p>
          <div className={styles.actions}>
            <Button
              variant="secondary"
              onClick={() => {
                window.location.hash = `#/journal/check-in?change=${item.id}`;
              }}
            >
              {t("item.checkin.change")}
            </Button>
            <Button variant="secondary" disabled={busy} onClick={() => void toggleLeftOut()}>
              {t(checkin.leftOut ? "item.checkin.bringBack" : "journal.timeline.leaveOut")}
            </Button>
            <Button variant="forget" onClick={() => setDeleting(true)}>
              {t("item.action.forget")}
            </Button>
          </div>
        </div>
      </div>
      {deleting ? (
        <DeleteCheckin
          itemId={item.id}
          feel={feel}
          time={time}
          hasLine={Boolean(checkin.line)}
          onClose={() => setDeleting(false)}
          onDone={() => {
            refresh("journal", "items", "memories", "entities", "page", "status");
            toast.show({ message: t("journal.delete.done") });
            navigate(`/journal/${checkin.day}`);
          }}
        />
      ) : null}
      {card ? (
        <MentionCard
          key={card.mention.id}
          mention={card.mention}
          anchor={card.anchor}
          onClose={() => setCard(null)}
        />
      ) : null}
    </ScreenSheet>
  );
}
