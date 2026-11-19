import { useCallback, useEffect, useRef, useState } from "react";
import { failureCopy, useRecorder } from "../../app/useRecorder";
import { useToast } from "../../components/Toast/Toast";
import { formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { EntryBlock, JournalDay, Mention } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { useEngine, useRefresh } from "../../engine/context";
import { FeelingMark } from "../../journal/FeelingMark";
import { MentionChip } from "../../journal/MentionChip";
import { usePlatform } from "../../platform/context";
import { Earlier, isUnread, PhotoBlock, VoiceBlock } from "./Blocks";
import styles from "./Entry.module.css";
import { GrowText } from "./GrowText";
import { Glyph } from "./glyphs";
import { clock12, weekdayOf } from "./time";

/** How long the person can pause before what they wrote is saved. */
const SAVE_AFTER_MS = 800;
/** How long Space is held in an empty paragraph before it starts recording. */
const HOLD_MS = 350;

interface Slot {
  key: string;
  block: EntryBlock | null;
  at: number;
}

interface EntryProps {
  data: JournalDay;
  /** The person or place whose words are outlined while its chip is hovered or focused. */
  highlight: string | null;
  onHighlight: (entityId: string | null) => void;
  openCard: (mention: Mention, anchor: HTMLElement) => void;
  cardMentionId: string | null;
  /** Opens the line for a good thing in the side column. */
  onAddGood: (() => void) | null;
}

const isUnsupported = (error: unknown) =>
  error instanceof EngineError &&
  error.code === "invalid" &&
  error.detail.includes("cannot be kept");

/** The names that are still standing, in the order the words come. */
const standing = (block: EntryBlock) => block.mentions.filter((m) => m.state !== "removed");

/** Where a name's words sit in the paragraph, when it was found word for word. */
const placed = (m: Mention): m is Mention & { start: number; end: number } =>
  m.start !== null && m.end !== null;

/**
 * Today's entry: paragraphs that save as the person writes, voice notes and photos between them,
 * and the people and places found in it once it has been read.
 */
export function Entry({
  data,
  highlight,
  onHighlight,
  openCard,
  cardMentionId,
  onAddGood,
}: EntryProps) {
  const engine = useEngine();
  const refresh = useRefresh();
  const toast = useToast();
  const platform = usePlatform();
  const { day } = data;
  const blocks = [...data.entry].sort((a, b) => a.block - b.block);

  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [extra, setExtra] = useState<{ key: string; at: number } | null>(null);
  const [tail, setTail] = useState(0);
  const [saved, setSaved] = useState("");
  const [busy, setBusy] = useState(false);

  /** A paragraph keeps its field, and so its focus, when saving gives it a new item. */
  const keys = useRef(new Map<string, string>());
  const latest = useRef(new Map<string, string | null>());
  const where = useRef(new Map<string, number>());
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const queue = useRef(new Map<string, Promise<unknown>>());
  const texts = useRef(new Map<string, string>());
  const focusNext = useRef<string | null>(null);
  const fields = useRef(new Map<string, HTMLTextAreaElement>());
  const blocksRef = useRef(blocks);
  blocksRef.current = blocks;

  const keyOf = (block: EntryBlock) => keys.current.get(block.item.id) ?? block.item.id;
  const tailKey = `tail-${tail}`;

  const save = useCallback(
    (key: string) => {
      if (!texts.current.has(key)) return Promise.resolve();
      const run = async () => {
        const text = texts.current.get(key);
        if (text === undefined) return;
        const current = blocksRef.current.find(
          (b) => (keys.current.get(b.item.id) ?? b.item.id) === key,
        );
        const itemId = latest.current.has(key)
          ? (latest.current.get(key) ?? null)
          : (current?.item.id ?? null);
        const at = current?.block ?? where.current.get(key) ?? blocksRef.current.length;
        try {
          const made = await engine.saveEntryText(day, itemId, at, text);
          if (texts.current.get(key) === text) texts.current.delete(key);
          if (made) {
            keys.current.set(made.item.id, key);
            latest.current.set(key, made.item.id);
          } else {
            latest.current.set(key, null);
          }
          setSaved(t("journal.a11y.saved"));
          refresh("journal");
        } catch (error) {
          // A save the engine refuses (emptying a block that was read) never succeeds: it is
          // said once, not retried.
          if (
            error instanceof EngineError &&
            error.code === "invalid" &&
            texts.current.get(key) === text
          ) {
            texts.current.delete(key);
          }
          toast.show({ message: t("memories.failed") });
        }
      };
      const next = (queue.current.get(key) ?? Promise.resolve()).then(run, run);
      queue.current.set(key, next);
      return next;
    },
    [engine, day, refresh, toast],
  );

  const flush = useCallback(() => {
    const waiting = new Set([...timers.current.keys(), ...texts.current.keys()]);
    for (const timer of timers.current.values()) clearTimeout(timer);
    timers.current.clear();
    return Promise.all([...waiting].map((key) => save(key)));
  }, [save]);

  const write = (key: string, text: string, at: number) => {
    setDrafts((all) => ({ ...all, [key]: text }));
    texts.current.set(key, text);
    where.current.set(key, at);
    clearTimeout(timers.current.get(key));
    timers.current.set(
      key,
      setTimeout(() => {
        timers.current.delete(key);
        void save(key);
      }, SAVE_AFTER_MS),
    );
  };

  const flushKey = (key: string) => {
    if (!timers.current.has(key) && !texts.current.has(key)) return;
    clearTimeout(timers.current.get(key));
    timers.current.delete(key);
    void save(key);
  };

  // Leaving the journal: what was written is saved, then the entry is read now.
  const leave = useRef({ flush, day, engine });
  leave.current = { flush, day, engine };
  useEffect(
    () => () => {
      const { flush: flushAll, day: when, engine: host } = leave.current;
      void flushAll().then(() => host.readEntryNow(when));
    },
    [],
  );

  const have = new Set(blocks.map(keyOf));
  useEffect(() => {
    if (extra && have.has(extra.key)) setExtra(null);
    if (have.has(tailKey)) setTail((n) => n + 1);
  });

  // A draft is dropped once the entry itself says the same words.
  useEffect(() => {
    setDrafts((all) => {
      let changed = false;
      const next = { ...all };
      for (const block of blocksRef.current) {
        const key = keys.current.get(block.item.id) ?? block.item.id;
        if (key in next && next[key] === (block.item.text ?? "") && !timers.current.has(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : all;
    });
  }, [data.entry]);

  const last = blocks.at(-1);
  const slots: Slot[] = blocks.map((block) => ({ key: keyOf(block), block, at: block.block }));
  let pending: { key: string; at: number } | null = null;
  if (extra && !have.has(extra.key)) pending = extra;
  else if ((last === undefined || last.item.kind !== "text") && !have.has(tailKey)) {
    pending = { key: tailKey, at: blocks.length };
  }
  if (pending) slots.splice(pending.at, 0, { key: pending.key, block: null, at: pending.at });

  // Voice and photos

  const place = useCallback(
    async (itemId: string, at: number) => {
      await engine.placeInEntry(itemId, day, at);
      setExtra(null);
      refresh("journal", "items", "status");
    },
    [engine, day, refresh],
  );

  const recordAt = useRef<number | null>(null);
  const keepVoice = useCallback(
    async (bytes: Uint8Array, mime: string, durationMs: number, at: number) => {
      try {
        const kept = await engine.keepBytes(bytes, mime, "journal");
        await place(kept.item.id, at);
        toast.show({
          message: t("capture.kept.toast", {
            kind: t("kind.audio"),
            duration: formatDuration(durationMs),
          }),
        });
      } catch {
        toast.show({
          message: t("capture.recording.failed"),
          action: {
            label: t("button.tryAgain"),
            onAction: () => void keepVoice(bytes, mime, durationMs, at),
          },
        });
      }
    },
    [engine, place, toast],
  );

  const recorder = useRecorder({
    onFinished: (bytes, mime, durationMs) => {
      const at = recordAt.current ?? blocksRef.current.length;
      recordAt.current = null;
      void keepVoice(bytes, mime, durationMs, at);
    },
    onFailed: (reason) => toast.show({ message: t(failureCopy(reason)), duration: 9000 }),
  });

  const addPhotos = async () => {
    const paths = await platform.pickFiles();
    if (paths.length === 0) return;
    setBusy(true);
    try {
      const kept = await engine.keepFiles(paths, "journal");
      let at = blocksRef.current.length;
      for (const one of kept) {
        await place(one.item.id, at);
        at += 1;
      }
    } catch (error) {
      toast.show({
        message: t(isUnsupported(error) ? "capture.addFile.unsupported" : "memories.failed"),
      });
    } finally {
      setBusy(false);
    }
  };

  const holding = useRef<ReturnType<typeof setTimeout> | null>(null);
  const talking = useRef(false);

  const keysFor = (slot: Slot, value: string) => ({
    onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.nativeEvent.isComposing) return;
      if (event.key === " " && value === "") {
        event.preventDefault();
        if (event.repeat || holding.current) return;
        holding.current = setTimeout(() => {
          holding.current = null;
          talking.current = true;
          recordAt.current = slot.at;
          recorder.start();
        }, HOLD_MS);
        return;
      }
      const field = event.currentTarget;
      if (
        event.key === "Enter" &&
        !event.shiftKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        value.trim() !== "" &&
        field.selectionStart === value.length &&
        field.selectionEnd === value.length
      ) {
        event.preventDefault();
        flushKey(slot.key);
        const key = `new-${Date.now()}-${slot.at}`;
        focusNext.current = key;
        setExtra({ key, at: slot.at + 1 });
      }
    },
    onKeyUp: (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.key !== " ") return;
      if (holding.current) {
        clearTimeout(holding.current);
        holding.current = null;
      }
      if (talking.current) {
        talking.current = false;
        recorder.stop();
      }
    },
  });

  useEffect(
    () => () => {
      if (holding.current) clearTimeout(holding.current);
    },
    [],
  );

  // Names

  const chips: Mention[] = blocks.flatMap((b) => (b.item.kind === "text" ? standing(b) : []));
  const read = data.entryReadAt !== null;
  const changedAt = blocks.reduce<string | null>(
    (latestAt, b) =>
      b.changedAt && (!latestAt || b.changedAt > latestAt) ? b.changedAt : latestAt,
    null,
  );
  const heading = data.today
    ? t("journal.entry.heading.today")
    : t("journal.entry.heading.day", { weekday: weekdayOf(day) });
  const unreadAny = blocks.length > 0 && blocks.some(isUnread);
  const meta = read
    ? t("journal.entry.read", { time: clock12(data.entryReadAt ?? "") })
    : data.entrySavedAt
      ? t("journal.entry.saved", { time: clock12(data.entrySavedAt) })
      : null;

  return (
    <section className={styles.entry} aria-labelledby="entry-heading">
      <div className={styles.head}>
        <h2 className={styles.label} id="entry-heading">
          {heading}
        </h2>
        <span className={styles.meta}>
          {changedAt ? (
            <span>{t("journal.entry.changed", { time: clock12(changedAt) })}</span>
          ) : null}
          {meta ? <span>{meta}</span> : null}
        </span>
      </div>
      <span className="sr-only" role="status">
        {saved}
      </span>

      <div className={styles.blocks}>
        {slots.map((slot, index) => {
          const block = slot.block;
          if (block && block.item.kind === "audio")
            return <VoiceBlock key={slot.key} block={block} />;
          if (block && block.item.kind === "image")
            return <PhotoBlock key={slot.key} block={block} />;
          const serverText = block?.item.text ?? "";
          const value = drafts[slot.key] ?? serverText;
          const marks =
            block && highlight && value === serverText
              ? standing(block)
                  .filter(placed)
                  .filter((m) => m.entityId === highlight)
                  .map((m) => ({ start: m.start, end: m.end }))
              : [];
          return (
            <div key={slot.key} className={styles.paragraph}>
              <GrowText
                value={value}
                label={t("journal.entry.paragraph", { number: index + 1 })}
                placeholder={
                  !block && slot.key === tailKey ? t("journal.entry.placeholder") : undefined
                }
                marks={marks}
                textRef={(el) => {
                  if (el) {
                    fields.current.set(slot.key, el);
                    if (focusNext.current === slot.key) {
                      focusNext.current = null;
                      el.focus();
                    }
                  } else {
                    fields.current.delete(slot.key);
                  }
                }}
                onChange={(text) => write(slot.key, text, slot.at)}
                onBlur={() => flushKey(slot.key)}
                {...keysFor(slot, value)}
              />
              {block ? <Earlier block={block} /> : null}
            </div>
          );
        })}
      </div>

      {chips.length > 0 ? (
        <ul className={styles.strip} aria-label={t("journal.people.strip")}>
          <li className={styles.stripLabel} aria-hidden="true">
            {t("journal.people.strip")}
          </li>
          {chips.map((m) => (
            <li key={m.id}>
              <MentionChip
                name={m.name}
                kind={m.kind}
                isNew={m.new}
                pressed={cardMentionId === m.id}
                onClick={(event) => openCard(m, event.currentTarget)}
                onMouseEnter={() => onHighlight(m.entityId)}
                onMouseLeave={() => onHighlight(null)}
                onFocus={() => onHighlight(m.entityId)}
                onBlur={() => onHighlight(null)}
              />
            </li>
          ))}
        </ul>
      ) : null}

      {unreadAny ? <p className={styles.small}>{t("journal.entry.reading")}</p> : null}

      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.tool}
          disabled={busy}
          onClick={() => void addPhotos()}
        >
          <Glyph name="image" />
          {t("journal.entry.addPhoto")}
        </button>
        <button
          type="button"
          className={styles.tool}
          aria-pressed={recorder.seconds !== null}
          aria-disabled={recorder.unavailable ? true : undefined}
          onClick={() => {
            if (recorder.seconds !== null) {
              recorder.stop();
            } else {
              recordAt.current = blocksRef.current.length;
              recorder.start();
            }
          }}
        >
          <Glyph name="mic" />
          {recorder.seconds !== null
            ? t("bar.listening", { time: formatDuration(recorder.seconds * 1000) })
            : t("journal.entry.record")}
        </button>
        <a className={styles.tool} href="#/journal/check-in">
          <FeelingMark mark="settled" size={16} tone={false} />
          {t("journal.entry.checkIn")}
        </a>
        {onAddGood ? (
          <button type="button" className={styles.tool} onClick={onAddGood}>
            <FeelingMark mark="sun" size={16} tone={false} />
            {t("journal.entry.addGood")}
          </button>
        ) : null}
      </div>
    </section>
  );
}
