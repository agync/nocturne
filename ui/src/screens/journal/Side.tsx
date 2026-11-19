import { useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { CheckIn, GoodThing, ItemView, JournalDay, Mention } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { FeelingMap } from "../../journal/FeelingMap";
import { FeelingMark } from "../../journal/FeelingMark";
import { PARTS } from "../../journal/feelings";
import { MentionChip } from "../../journal/MentionChip";
import { usePlatform } from "../../platform/context";
import { Glyph } from "./glyphs";
import styles from "./Side.module.css";
import { clock12 } from "./time";

const MOST_GOOD_THINGS = 3;

interface SideProps {
  data: JournalDay;
  openCard: (mention: Mention, anchor: HTMLElement) => void;
  cardMentionId: string | null;
  onHighlight: (entityId: string | null) => void;
  adding: boolean;
  setAdding: (adding: boolean) => void;
  addRef: React.RefObject<HTMLInputElement | null>;
}

const words = (c: CheckIn) => c.feeling.words.join(", ");

const dotLabel = (c: CheckIn) =>
  `${t(PARTS[c.feeling.part].name)}, ${words(c)}, ${clock12(c.keptAt)}`;

/** Every standing mention in the day, for finding the one a name in the side column stands for. */
function allMentions(data: JournalDay): Mention[] {
  return [
    ...data.entry.flatMap((b) => b.mentions),
    ...data.rows.flatMap((r) => r.mentions),
    ...data.checkins.flatMap((c) => c.mentions),
    ...data.good.flatMap((g) => g.mentions),
  ].filter((m) => m.state !== "removed");
}

/** A photo kept today, for "Use a photo from today". */
function photosOf(data: JournalDay): ItemView[] {
  const seen = new Map<string, ItemView>();
  for (const b of data.entry) if (b.item.kind === "image") seen.set(b.item.id, b.item);
  for (const r of data.rows) if (r.kind === "photo") seen.set(r.item.id, r.item);
  return [...seen.values()];
}

const nameOf = (item: ItemView) => item.summary ?? item.title;

function GoodLine({ good }: { good: GoodThing }) {
  const platform = usePlatform();
  const src = good.photo?.originalPath ? platform.originalSrc(good.photo.originalPath) : null;
  return (
    <li className={styles.good}>
      <FeelingMark mark="sun" size={16} tone={false} />
      <span>{good.text}</span>
      {src ? <img className={styles.goodPhoto} src={src} alt="" /> : null}
    </li>
  );
}

function AddGood({
  data,
  inputRef,
  onDone,
}: {
  data: JournalDay;
  inputRef: React.RefObject<HTMLInputElement | null>;
  onDone: () => void;
}) {
  const engine = useEngine();
  const platform = usePlatform();
  const refresh = useRefresh();
  const toast = useToast();
  const [text, setText] = useState("");
  const [photo, setPhoto] = useState<ItemView | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [busy, setBusy] = useState(false);
  const today = photosOf(data);

  const pick = async () => {
    const paths = await platform.pickFiles();
    const first = paths[0];
    if (!first) return;
    try {
      const kept = await engine.keepFiles([first], "journal");
      const item = kept[0]?.item;
      if (item) setPhoto(item);
    } catch {
      toast.show({ message: t("memories.failed") });
    }
  };

  const save = async () => {
    const line = text.trim();
    if (!line) return;
    setBusy(true);
    try {
      await engine.keepGoodThing(data.day, line, photo?.id ?? null);
      refresh("journal", "items", "status");
      onDone();
    } catch {
      toast.show({ message: t("memories.failed") });
      setBusy(false);
    }
  };

  return (
    <form
      className={styles.add}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onDone();
        }
      }}
    >
      <input
        ref={inputRef}
        className={styles.line}
        value={text}
        maxLength={280}
        aria-label={t("journal.side.addGood")}
        placeholder={t("journal.good.placeholder")}
        onChange={(event) => setText(event.target.value)}
      />
      <div className={styles.photoRow}>
        <button type="button" className={styles.quiet} onClick={() => void pick()}>
          <Glyph name="image" />
          {t("journal.good.photoAdd")}
        </button>
        {today.length > 0 ? (
          <button
            type="button"
            className={styles.quiet}
            aria-expanded={choosing}
            onClick={() => setChoosing(!choosing)}
          >
            {t("journal.good.photoUse")}
          </button>
        ) : null}
      </div>
      {choosing ? (
        <ul className={styles.choices}>
          {today.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                className={styles.choice}
                aria-pressed={photo?.id === item.id}
                onClick={() => {
                  setPhoto(item);
                  setChoosing(false);
                }}
              >
                <Glyph name="image" />
                {nameOf(item)}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {photo ? (
        <p className={styles.chosen}>{t("journal.good.photoChosen", { name: nameOf(photo) })}</p>
      ) : null}
      <div className={styles.actions}>
        <Button type="submit" variant="primary" disabled={busy || text.trim() === ""}>
          {t("journal.checkin.save")}
        </Button>
        <Button variant="quiet" disabled={busy} onClick={onDone}>
          {t("dialog.cancel")}
        </Button>
      </div>
    </form>
  );
}

/** How the day felt, its good things, who and where it mentions, and what was written about it. */
export function Side({
  data,
  openCard,
  cardMentionId,
  onHighlight,
  adding,
  setAdding,
  addRef,
}: SideProps) {
  const open = useRef<HTMLButtonElement>(null);
  const mentions = allMentions(data);
  const full = data.good.length >= MOST_GOOD_THINGS;
  const emptyDay = data.entry.length === 0 && data.rows.length === 0;

  return (
    <aside className={styles.side} aria-label={t("journal.day.eyebrow")}>
      <section className={styles.block} aria-labelledby="side-felt">
        <h2 className={styles.label} id="side-felt">
          {t("journal.side.felt")}
        </h2>
        {data.checkins.length > 0 ? (
          <>
            <FeelingMap
              checkins={data.checkins}
              size={220}
              onOpen={(c) => {
                goTo(`/items/${c.id}`);
              }}
              dotLabel={dotLabel}
            />
            <ul className={styles.list}>
              {data.checkins.map((c) => (
                <li key={c.id} className={styles.feelRow}>
                  <span className={styles.feel} data-part={c.feeling.part}>
                    <FeelingMark mark={c.feeling.part} size={14} />
                    {words(c)}
                  </span>
                  <span className={styles.meta}>{clock12(c.keptAt)}</span>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p className={styles.small}>{t("journal.side.feltEmpty")}</p>
        )}
        <a className={styles.secondary} href="#/journal/check-in">
          {t("journal.side.checkIn")}
        </a>
      </section>

      {emptyDay && !adding ? null : (
        <section className={styles.block} aria-labelledby="side-good">
          <h2 className={styles.label} id="side-good">
            {t("journal.side.good")}
          </h2>
          {data.good.length > 0 ? (
            <ul className={styles.list}>
              {data.good.map((g) => (
                <GoodLine key={g.id} good={g} />
              ))}
            </ul>
          ) : null}
          {adding && !full ? (
            <AddGood
              data={data}
              inputRef={addRef}
              onDone={() => {
                setAdding(false);
                open.current?.focus();
              }}
            />
          ) : full ? null : (
            <button
              ref={open}
              type="button"
              className={styles.secondary}
              onClick={() => setAdding(true)}
            >
              <Glyph name="plus" />
              {t("journal.side.addGood")}
            </button>
          )}
        </section>
      )}

      {data.people.length > 0 ? (
        <section className={styles.block} aria-labelledby="side-people">
          <h2 className={styles.label} id="side-people">
            {t("journal.side.people")}
          </h2>
          <ul className={styles.chips}>
            {data.people.map((ref) => {
              const mention =
                mentions.find(
                  (m) => m.entityId === ref.entityId && ref.itemIds.includes(m.itemId),
                ) ?? mentions.find((m) => m.entityId === ref.entityId);
              return (
                <li key={ref.entityId}>
                  <MentionChip
                    name={ref.name}
                    kind={ref.kind}
                    isNew={ref.new}
                    pressed={mention !== undefined && cardMentionId === mention.id}
                    disabled={mention === undefined}
                    onClick={(event) => mention && openCard(mention, event.currentTarget)}
                    onMouseEnter={() => onHighlight(ref.entityId)}
                    onMouseLeave={() => onHighlight(null)}
                    onFocus={() => onHighlight(ref.entityId)}
                    onBlur={() => onHighlight(null)}
                  />
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      {data.reflection ? (
        <section className={styles.block} aria-labelledby="side-reflection">
          <h2 className={styles.label} id="side-reflection">
            {t("journal.side.reflection")}
          </h2>
          <p className={styles.quote}>{data.reflection.firstSentence}</p>
          <span className={styles.meta}>
            {t("journal.side.reflectionMeta", {
              time: clock12(data.reflection.writtenAt),
              count: data.reflection.fromCount,
            })}
          </span>
          <a className={styles.open} href={`#/journal/look-back/day/${data.day}`}>
            {t("journal.side.open")}
          </a>
        </section>
      ) : null}
    </aside>
  );
}
