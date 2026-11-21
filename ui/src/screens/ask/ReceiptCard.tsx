import { useEffect, useRef, useState } from "react";
import { Thumb } from "../../components/Thumb/Thumb";
import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { Receipt } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import styles from "./Ask.module.css";

function kindWord(receipt: Receipt): string {
  switch (receipt.kind) {
    case "audio":
      return t("kind.audio");
    case "image":
      return t("kind.image");
    case "document":
      return t("kind.document");
    case "web":
      return t("kind.web");
    case "checkin":
      return t("kind.checkin");
    case "calendar":
      return t("kind.calendar");
    case "reading":
      return t("kind.reading");
    case "text":
      return receipt.source === "paste" ? t("kind.paste") : t("kind.text");
  }
}

/** "Photo · Wed 15 Oct", "Voice note · 0:10 · Tue 14 Oct", "Document · p.2". */
export function receiptMeta(receipt: Receipt): string {
  const parts = [kindWord(receipt)];
  if (receipt.kind === "audio" && receipt.durationMs !== null) {
    parts.push(formatDuration(receipt.durationMs));
  }
  parts.push(
    receipt.span?.type === "page"
      ? t("receipt.page", { page: receipt.span.page })
      : formatDay(receipt.keptAt),
  );
  return parts.join(" · ");
}

/** What the original said at this spot, when the engine knows it. */
function quote(receipt: Receipt): string | null {
  return receipt.span?.type === "quote" ? `“${receipt.span.text}”` : null;
}

interface ReceiptCardProps {
  receipt: Receipt;
  /** An answer span the pointer or keyboard is on points here. */
  active?: boolean;
  onOpen: (receipt: Receipt) => void;
}

/** A receipt as a card: the original's title, what it said, and for a voice note a play button. */
export function ReceiptCard({ receipt, active = false, onOpen }: ReceiptCardProps) {
  const engine = useEngine();
  const platform = usePlatform();
  const [playing, setPlaying] = useState(false);
  const audio = useRef<HTMLAudioElement | null>(null);
  const stopAt = useRef<number | null>(null);

  useEffect(
    () => () => {
      audio.current?.pause();
    },
    [],
  );

  const toggle = async () => {
    if (playing) {
      audio.current?.pause();
      setPlaying(false);
      return;
    }
    try {
      if (!audio.current) {
        const item = await engine.getItem(receipt.itemId);
        if (!item.originalPath) return;
        const element = new Audio(platform.originalSrc(item.originalPath));
        element.addEventListener("ended", () => setPlaying(false));
        element.addEventListener("error", () => setPlaying(false));
        element.addEventListener("timeupdate", () => {
          if (stopAt.current !== null && element.currentTime * 1000 >= stopAt.current) {
            element.pause();
            setPlaying(false);
          }
        });
        audio.current = element;
      }
      if (receipt.span?.type === "time") {
        audio.current.currentTime = receipt.span.startMs / 1000;
        stopAt.current = receipt.span.endMs;
      }
      await audio.current.play();
      setPlaying(true);
    } catch {
      setPlaying(false);
    }
  };

  const said = quote(receipt);
  return (
    <li
      className={styles.receipt}
      data-kind={receipt.kind}
      data-active={active}
      data-unconfirmed={receipt.unconfirmed}
      data-playing={playing}
    >
      {receipt.kind === "audio" ? (
        <button
          type="button"
          className={styles.play}
          aria-pressed={playing}
          aria-label={t(playing ? "ask.receipt.pause" : "ask.receipt.play", {
            title: receipt.label,
          })}
          onClick={() => void toggle()}
        >
          <span className={styles.playGlyph} data-playing={playing} aria-hidden="true" />
        </button>
      ) : (
        <span className={styles.thumb} data-kind={receipt.kind}>
          <Thumb kind={receipt.kind} size={receipt.kind === "image" ? 40 : 18} />
        </span>
      )}
      <button
        type="button"
        className={styles.receiptOpen}
        data-open
        onClick={() => onOpen(receipt)}
      >
        <span className={styles.receiptTitle}>{receipt.label}</span>
        {said ? <span className={styles.receiptSaid}>{said}</span> : null}
        <span className={styles.receiptMeta}>{receiptMeta(receipt)}</span>
      </button>
    </li>
  );
}
