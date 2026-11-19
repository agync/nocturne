import { useCallback, useEffect, useRef, useState } from "react";
import { useRoute } from "wouter";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";
import type { Mention } from "../../engine/bindings";
import {
  useEngine,
  useJournalDay,
  useJournalDays,
  useRefresh,
  useStatus,
} from "../../engine/context";
import { MentionCard } from "../../journal/MentionCard";
import { NotFound } from "../NotFound";
import { DayPicker } from "./DayPicker";
import { Entry } from "./Entry";
import { Glyph } from "./glyphs";
import styles from "./Journal.module.css";
import { Side } from "./Side";
import { Timeline } from "./Timeline";
import { addDays, dayWords, isDay, weekdayOf } from "./time";

/** Serves #/journal (today) and #/journal/:day. */
export default function Journal() {
  const [, params] = useRoute("/journal/:day");
  const day = params?.day;
  if (day !== undefined && !isDay(day)) return <NotFound />;
  return <JournalPage key={day ?? "today"} day={day} />;
}

function JournalPage({ day: routeDay }: { day: string | undefined }) {
  const engine = useEngine();
  const refresh = useRefresh();
  const today = useJournalDay();
  const found = useJournalDay(routeDay);
  const days = useJournalDays();
  const status = useStatus();
  const [picking, setPicking] = useState(false);
  /** The day's items as they were when "Delete this day…" was chosen. */
  const [deleting, setDeleting] = useState<string[] | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [highlight, setHighlight] = useState<string | null>(null);
  const [card, setCard] = useState<{ mention: Mention; anchor: HTMLElement } | null>(null);
  const [adding, setAdding] = useState(false);
  const addRef = useRef<HTMLInputElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  const data = found.data;
  const todayDay = today.data?.day;

  useEffect(() => {
    if (adding) addRef.current?.focus();
  }, [adding]);

  useEffect(() => {
    if (!menuOpen) return;
    const away = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [menuOpen]);

  const openCard = useCallback((mention: Mention, anchor: HTMLElement) => {
    setCard({ mention, anchor });
  }, []);
  const closeCard = useCallback(() => {
    setCard(null);
    setHighlight(null);
  }, []);

  if (routeDay !== undefined && todayDay !== undefined && routeDay > todayDay) return <NotFound />;

  if (!data) {
    return (
      <ScreenSheet>
        <p className={styles.eyebrow}>{t("journal.day.eyebrow")}</p>
        {found.isError ? (
          <div className={styles.problem} role="alert">
            <span>{t("journal.timeline.error")}</span>
            <button type="button" className={styles.retry} onClick={() => void found.refetch()}>
              {t("button.tryAgain")}
            </button>
          </div>
        ) : (
          <p className={styles.loading} role="status">
            {t("memories.loading")}
          </p>
        )}
      </ScreenSheet>
    );
  }

  const empty = data.entry.length === 0 && data.rows.length === 0;
  const earlier = (days.data ?? []).filter((d) => d < data.day).at(-1) ?? addDays(data.day, -1);
  const words = dayWords(data.day);
  const notReady = status.data?.state === "gettingReady" || status.data?.state === "starting";
  const canAddGood = data.good.length < 3;
  const itemIds = [
    ...new Set([
      ...data.entry.flatMap((b) => [b.item.id, ...b.earlier.map((e) => e.itemId)]),
      ...data.rows.map((r) => r.item.id),
      ...data.checkins.map((c) => c.id),
      ...data.good.map((g) => g.id),
    ]),
  ];

  return (
    <ScreenSheet>
      <div className={styles.head}>
        <div className={styles.headText}>
          <p className={styles.eyebrow}>{t("journal.day.eyebrow")}</p>
          <h1 className={`t-page-title ${styles.title}`}>
            {t("journal.day.title", { weekday: words.weekday, day: words.day, month: words.month })}
          </h1>
          <div className={styles.daynav}>
            <a className={styles.navLink} href={`#/journal/${earlier}`}>
              {t("journal.day.previous", { weekday: weekdayOf(earlier) })}
            </a>
            <button type="button" className={styles.navLink} onClick={() => setPicking(true)}>
              <Glyph name="calendar" />
              {t("journal.day.choose")}
            </button>
            <div className={styles.dayMenu} ref={menu}>
              <button
                type="button"
                className={styles.navLink}
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                aria-label={t("journal.day.menu")}
                onClick={() => setMenuOpen(!menuOpen)}
              >
                <Glyph name="more" />
              </button>
              {menuOpen ? (
                <div className={styles.dayMenuList} role="menu" aria-label={t("journal.day.menu")}>
                  <button
                    type="button"
                    role="menuitem"
                    className={styles.dayMenuItem}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") setMenuOpen(false);
                    }}
                    onClick={() => {
                      setMenuOpen(false);
                      setDeleting(itemIds);
                    }}
                  >
                    {t("journal.day.deleteDay")}
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </div>
        <div className={styles.reflect}>
          {empty ? (
            <>
              <button
                type="button"
                className={styles.primary}
                disabled
                aria-describedby="reflect-hint"
              >
                {t(data.today ? "journal.day.reflect" : "journal.day.reflectPast")}
              </button>
              <span className={styles.hint} id="reflect-hint">
                {t("journal.day.reflectEmpty")}
              </span>
            </>
          ) : (
            <a className={styles.primary} href={`#/journal/look-back/day/${data.day}?write=1`}>
              {t(data.today ? "journal.day.reflect" : "journal.day.reflectPast")}
            </a>
          )}
        </div>
      </div>

      {data.firstUse ? (
        <FirstUse
          onDismiss={() => {
            void engine.dismissJournalIntro().then(() => refresh("journal"));
          }}
        />
      ) : null}

      <div className={styles.cols}>
        <div className={styles.main}>
          <Entry
            data={data}
            highlight={highlight}
            onHighlight={setHighlight}
            openCard={openCard}
            cardMentionId={card?.mention.id ?? null}
            onAddGood={canAddGood ? () => setAdding(true) : null}
          />
          <Timeline
            data={data}
            notReady={notReady}
            failed={found.isError}
            onRetry={() => void found.refetch()}
            openCard={openCard}
            cardMentionId={card?.mention.id ?? null}
            onHighlight={setHighlight}
          />
        </div>
        <Side
          data={data}
          openCard={openCard}
          cardMentionId={card?.mention.id ?? null}
          onHighlight={setHighlight}
          adding={adding}
          setAdding={setAdding}
          addRef={addRef}
        />
      </div>

      {picking ? (
        <DayPicker today={todayDay ?? data.day} onClose={() => setPicking(false)} />
      ) : null}
      {deleting ? (
        <ForgetDialog
          memoryIds={[]}
          itemIds={deleting}
          pick
          defaultIncludeOriginals
          onDone={() => refresh("journal", "items", "memories", "entities", "page", "status")}
          onClose={() => setDeleting(null)}
        />
      ) : null}
      {card ? (
        <MentionCard
          key={card.mention.id}
          mention={card.mention}
          anchor={card.anchor}
          onClose={closeCard}
        />
      ) : null}
    </ScreenSheet>
  );
}

function FirstUse({ onDismiss }: { onDismiss: () => void }) {
  return (
    <section className={styles.first} aria-labelledby="first-title">
      <strong id="first-title">{t("journal.first.title")}</strong>
      <p>{t("journal.first.body")}</p>
      <button type="button" className={styles.gotIt} onClick={onDismiss}>
        {t("journal.first.button")}
      </button>
    </section>
  );
}
