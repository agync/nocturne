import { type ReactNode, useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Icon } from "../../components/Icon/Icon";
import { formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { ItemView, Span } from "../../engine/bindings";
import { usePlatform } from "../../platform/context";
import styles from "./Item.module.css";
import { usePeaks } from "./peaks";

export interface Highlight {
  span: Span;
  sentence: string;
}

interface OriginalProps {
  item: ItemView;
  highlights: Highlight[];
}

const clamp = (n: number) => Math.min(100, Math.max(0, n * 100));

/** Splits text around the quotes to highlight, in the order they appear. */
export function splitQuotes(
  text: string,
  quotes: string[],
): Array<{ text: string; mark: boolean }> {
  const wanted = quotes.map((q) => q.replace(/\.\.\.$|…$/, "").trim()).filter(Boolean);
  const parts: Array<{ text: string; mark: boolean }> = [];
  let rest = text;
  while (rest.length > 0) {
    let at = -1;
    let length = 0;
    for (const q of wanted) {
      const found = rest.indexOf(q);
      if (found !== -1 && (at === -1 || found < at)) {
        at = found;
        length = q.length;
      }
    }
    if (at === -1) {
      parts.push({ text: rest, mark: false });
      break;
    }
    if (at > 0) parts.push({ text: rest.slice(0, at), mark: false });
    parts.push({ text: rest.slice(at, at + length), mark: true });
    rest = rest.slice(at + length);
  }
  return parts;
}

function Paper({ children }: { children: ReactNode }) {
  return <div className={styles.paper}>{children}</div>;
}

function Image({ item, highlights }: OriginalProps) {
  const platform = usePlatform();
  const [failed, setFailed] = useState(false);
  const regions = highlights.filter((h) => h.span.type === "region");
  if (!item.originalPath || failed) {
    return (
      <Paper>
        <p className={styles.unavailable}>{t("item.original.unavailable")}</p>
      </Paper>
    );
  }
  return (
    <figure className={styles.figure}>
      <div className={styles.photo}>
        <img
          src={platform.originalSrc(item.originalPath)}
          alt={t("item.image.alt", { title: item.title })}
          onError={() => setFailed(true)}
        />
        {regions.map((h, i) =>
          h.span.type === "region" ? (
            <span
              key={`${i}-${h.sentence}`}
              role="img"
              className={styles.region}
              aria-label={t("item.image.highlight", { sentence: h.sentence })}
              style={{
                left: `${clamp(h.span.x)}%`,
                top: `${clamp(h.span.y)}%`,
                width: `${clamp(h.span.w)}%`,
                height: `${clamp(h.span.h)}%`,
              }}
            />
          ) : null,
        )}
      </div>
    </figure>
  );
}

function Audio({ item, highlights }: OriginalProps) {
  const platform = usePlatform();
  const audio = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [now, setNow] = useState(0);
  const [length, setLength] = useState(item.durationMs ?? 0);
  const [failed, setFailed] = useState(false);
  const tried = useRef(false);
  const src = item.originalPath ? platform.originalSrc(item.originalPath) : null;
  const peaks = usePeaks(src, 72, item.durationMs);
  const bars = peaks ?? Array.from({ length: 72 }, () => 0.35);
  const times = highlights.filter((h) => h.span.type === "time");

  useEffect(() => {
    const el = audio.current;
    if (!el) return;
    const tick = () => setNow(Math.round(el.currentTime * 1000));
    const meta = () => {
      if (Number.isFinite(el.duration) && el.duration > 0)
        setLength(Math.round(el.duration * 1000));
    };
    const end = () => setPlaying(false);
    // A file that will not load is only worth a word once the person has asked to hear it.
    const bad = () => {
      if (!tried.current) return;
      setFailed(true);
      setPlaying(false);
    };
    el.addEventListener("timeupdate", tick);
    el.addEventListener("loadedmetadata", meta);
    el.addEventListener("ended", end);
    el.addEventListener("error", bad);
    return () => {
      el.removeEventListener("timeupdate", tick);
      el.removeEventListener("loadedmetadata", meta);
      el.removeEventListener("ended", end);
      el.removeEventListener("error", bad);
    };
  }, []);

  if (!src) {
    return (
      <Paper>
        <p className={styles.unavailable}>{t("item.original.unavailable")}</p>
      </Paper>
    );
  }

  const toggle = async () => {
    const el = audio.current;
    if (!el) return;
    if (playing) {
      el.pause();
      setPlaying(false);
      return;
    }
    tried.current = true;
    try {
      await el.play();
      setPlaying(true);
    } catch {
      setFailed(true);
    }
  };

  const total = length || 1;
  return (
    <div className={styles.audio}>
      {/* biome-ignore lint/a11y/useMediaCaption: this is the person's own recording; its words are on the right */}
      <audio ref={audio} src={src} preload="metadata" />
      <div className={styles.wave} aria-hidden="true" data-measured={peaks !== null}>
        {bars.map((p, i) => {
          const at = ((i + 0.5) / bars.length) * total;
          const marked = times.some(
            (h) => h.span.type === "time" && at >= h.span.startMs && at <= h.span.endMs,
          );
          return (
            <span
              key={i}
              className={styles.bar}
              data-marked={marked}
              data-played={at <= now}
              style={{ height: `${Math.max(6, p * 100)}%` }}
            />
          );
        })}
      </div>
      <div className={styles.transport}>
        <Button variant="secondary" onClick={() => void toggle()}>
          <Icon name={playing ? "pause" : "play"} size={16} />
          <span>{playing ? t("item.audio.pause") : t("item.audio.play")}</span>
        </Button>
        <input
          type="range"
          className={styles.seek}
          min={0}
          max={total}
          step={500}
          value={Math.min(now, total)}
          aria-label={t("item.audio.position")}
          aria-valuetext={t("item.audio.positionText", {
            now: formatDuration(now),
            total: formatDuration(total),
          })}
          onChange={(event) => {
            const next = Number(event.target.value);
            setNow(next);
            if (audio.current) audio.current.currentTime = next / 1000;
          }}
        />
        <span className={styles.time}>
          {formatDuration(now)} / {formatDuration(total)}
        </span>
      </div>
      {failed ? (
        <p className={styles.unavailable} role="status">
          {t("item.audio.error")}
        </p>
      ) : null}
      {times.length > 0 ? (
        <section aria-label={t("item.highlights")} className={styles.highlights}>
          <h3 className="t-section">{t("item.highlights")}</h3>
          <ul>
            {times.map((h, i) =>
              h.span.type === "time" ? (
                <li key={`${i}-${h.sentence}`}>
                  {t("item.audio.highlight", {
                    from: formatDuration(h.span.startMs),
                    to: formatDuration(h.span.endMs),
                    sentence: h.sentence,
                  })}
                </li>
              ) : null,
            )}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

function Document({ item, highlights }: OriginalProps) {
  const body = item.text ?? item.summary;
  if (!body) {
    return (
      <Paper>
        <p className={styles.unavailable}>{t("item.paper.empty")}</p>
      </Paper>
    );
  }
  const quotes = highlights.flatMap((h) => (h.span.type === "quote" ? [h.span.text] : []));
  const parts = splitQuotes(body, quotes);
  return (
    <Paper>
      <p className={styles.paperText}>
        {parts.map((part, i) =>
          part.mark ? (
            <mark key={i} className={styles.quote}>
              <span className="sr-only">{t("item.paper.highlighted")}</span>
              {part.text}
            </mark>
          ) : (
            <span key={i}>{part.text}</span>
          ),
        )}
      </p>
    </Paper>
  );
}

/** The thing itself: the photo with its highlights, the recording with its waveform, or the words. */
export function Original({ item, highlights }: OriginalProps) {
  switch (item.kind) {
    case "image":
      return <Image item={item} highlights={highlights} />;
    case "audio":
      return <Audio item={item} highlights={highlights} />;
    default:
      return <Document item={item} highlights={highlights} />;
  }
}
