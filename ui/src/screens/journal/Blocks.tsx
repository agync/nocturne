import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { Icon } from "../../components/Icon/Icon";
import { TextInput } from "../../components/TextInput/TextInput";
import { formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { EntryBlock } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import styles from "./Entry.module.css";
import { Glyph } from "./glyphs";
import { clock12 } from "./time";

export const isUnread = (block: EntryBlock) =>
  block.item.status === "kept" || block.item.status === "reading";

/** A voice note in the entry: its length, play, and the words once it has been read. */
export function VoiceBlock({ block }: { block: EntryBlock }) {
  const platform = usePlatform();
  const audio = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const src = block.item.originalPath ? platform.originalSrc(block.item.originalPath) : null;
  const words = block.item.text?.trim();

  const toggle = async () => {
    const el = audio.current;
    if (!el) return;
    if (playing) {
      el.pause();
      setPlaying(false);
      return;
    }
    try {
      await el.play();
      setPlaying(true);
    } catch {
      setPlaying(false);
    }
  };

  return (
    <div className={styles.voice}>
      <div className={styles.voiceTop}>
        <button
          type="button"
          className={styles.play}
          aria-label={t(playing ? "item.audio.pause" : "item.audio.play")}
          aria-disabled={src ? undefined : true}
          onClick={() => void toggle()}
        >
          <Icon name={playing ? "pause" : "play"} size={14} />
        </button>
        <span>
          {t("journal.entry.voice", { duration: formatDuration(block.item.durationMs ?? 0) })}
        </span>
        <span className={styles.wave} aria-hidden="true" />
        {src ? (
          // biome-ignore lint/a11y/useMediaCaption: the person's own recording; its words follow
          <audio ref={audio} src={src} preload="none" onEnded={() => setPlaying(false)} />
        ) : null}
      </div>
      {isUnread(block) ? (
        <span className={styles.status}>{t("item.status.reading")}</span>
      ) : words ? (
        <Disclosure label={t("journal.entry.voiceWords")}>
          <p className={styles.said}>{words}</p>
        </Disclosure>
      ) : null}
    </div>
  );
}

/** A photo in the entry, with its description as its alternative text, which can be changed. */
export function PhotoBlock({ block }: { block: EntryBlock }) {
  const engine = useEngine();
  const refresh = useRefresh();
  const platform = usePlatform();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  const [failed, setFailed] = useState(false);
  const unread = isUnread(block);
  const description = block.alt ?? block.item.summary ?? "";
  const src = block.item.originalPath ? platform.originalSrc(block.item.originalPath) : null;

  const save = async () => {
    try {
      await engine.describePhoto(block.item.id, text.trim());
      refresh("journal");
      setEditing(false);
    } catch {
      setFailed(true);
    }
  };

  return (
    <figure className={styles.photo}>
      {src ? (
        <img className={styles.image} src={src} alt={description || t("journal.entry.photoAlt")} />
      ) : (
        <div className={styles.photoBox} role="img" aria-label={description || t("kind.image")}>
          <Glyph name="image" size={28} />
        </div>
      )}
      <figcaption className={styles.caption}>
        {unread ? t("journal.entry.photoUnread") : description}
      </figcaption>
      {unread ? null : editing ? (
        <form
          className={styles.describe}
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <TextInput
            label={t("journal.entry.describe")}
            value={text}
            maxLength={300}
            error={failed ? t("memories.failed") : undefined}
            onChange={setText}
          />
          <div className={styles.describeActions}>
            <Button type="submit" variant="secondary">
              {t("journal.checkin.save")}
            </Button>
            <Button variant="quiet" onClick={() => setEditing(false)}>
              {t("dialog.cancel")}
            </Button>
          </div>
        </form>
      ) : (
        <Button
          variant="quiet"
          flush
          onClick={() => {
            setText(description);
            setFailed(false);
            setEditing(true);
          }}
        >
          {t("journal.entry.describe")}
        </Button>
      )}
    </figure>
  );
}

/** "Earlier version": what a read paragraph said before it was changed. */
export function Earlier({ block }: { block: EntryBlock }) {
  if (block.earlier.length === 0) return null;
  return (
    <Disclosure label={t("journal.entry.earlier")}>
      <ul className={styles.earlier}>
        {block.earlier.map((version) => (
          <li key={version.itemId}>
            <span className={styles.status}>{clock12(version.keptAt)}</span>
            <span>{version.text ?? ""}</span>
          </li>
        ))}
      </ul>
    </Disclosure>
  );
}
