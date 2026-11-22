import { useEffect, useRef, useState } from "react";
import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { PhoneItem } from "../../engine/bindings";
import { ConnectionError, type PhoneEngine, PhoneRequestError } from "../engine";
import { Switch } from "../native/screens/parts";
import { renderingFor } from "../renderable";
import { Button, Icon, Page } from "./ui";

type Load =
  | { name: "loading" }
  | { name: "ready"; item: PhoneItem; blob: Blob | null }
  | { name: "error"; why: "private" | "gone" | "failed" };

/** An object URL that is made when a blob arrives and revoked when it goes or the screen does. */
export function useBlobUrl(blob: Blob | null): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!blob) return setUrl(null);
    const made = URL.createObjectURL(blob);
    setUrl(made);
    return () => URL.revokeObjectURL(made);
  }, [blob]);
  return url;
}

const KIND = {
  text: "kind.text",
  image: "kind.image",
  audio: "kind.audio",
  document: "kind.document",
  web: "kind.web",
  checkin: "kind.checkin",
  calendar: "kind.calendar",
  reading: "kind.reading",
} as const;

interface OriginalProps {
  engine: PhoneEngine;
  itemId: string;
  /** The words of the answer that used this, to be marked in a note. */
  quote: string | null;
  onBack(): void;
  /** A phone alone opens its own memory, and a thing kept can be marked private here. */
  alone?: AloneItem;
}

export interface AloneItem {
  setPrivate(id: string, on: boolean): Promise<PhoneItem>;
  /** Opened from Ask: the marked words are what was looked for. */
  found: boolean;
}

export function Original({ engine, itemId, quote, onBack, alone }: OriginalProps) {
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [attempt, setAttempt] = useState(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const blob = load.name === "ready" ? load.blob : null;
  const rendering = blob ? renderingFor(blob.type) : null;
  // A type that cannot be shown safely never gets a URL.
  const url = useBlobUrl(rendering ? blob : null);

  useEffect(() => {
    void attempt;
    let gone = false;
    setLoad({ name: "loading" });
    (async () => {
      try {
        const item = await engine.getItem(itemId);
        const wantsBlob = item.hasOriginal && item.kind !== "text" && item.kind !== "web";
        const original = wantsBlob ? await engine.original(itemId) : null;
        if (!gone) setLoad({ name: "ready", item, blob: original });
      } catch (e) {
        if (gone) return;
        if (e instanceof PhoneRequestError && e.code === "notYours")
          return setLoad({ name: "error", why: "private" });
        if (e instanceof PhoneRequestError && e.code === "notFound")
          return setLoad({ name: "error", why: "gone" });
        if (e instanceof ConnectionError || e instanceof PhoneRequestError)
          return setLoad({ name: "error", why: "failed" });
        throw e;
      }
    })();
    return () => {
      gone = true;
    };
  }, [engine, itemId, attempt]);

  useEffect(() => {
    if (load.name !== "loading") heading.current?.focus();
  }, [load.name]);

  const bar = (
    <header className="ph-bar ph-bar--back">
      {alone ? (
        <button
          type="button"
          className="ph-ib ph-ib--ink"
          onClick={onBack}
          aria-label={t("restore.back")}
        >
          <Icon name="back" />
        </button>
      ) : (
        <button type="button" className="ph-ib ph-ib--ink" onClick={onBack}>
          <Icon name="back" />
          {t("phone.original.back")}
        </button>
      )}
    </header>
  );

  if (load.name === "loading") {
    return (
      <Page bar={bar}>
        <main className="ph-main" aria-busy="true">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {alone ? t("phone.alone.original.loading") : t("phone.original.loading")}
          </p>
        </main>
      </Page>
    );
  }
  if (load.name === "error") {
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <h1 className="ph-h1 ph-h1--small" role="alert" tabIndex={-1} ref={heading}>
            {load.why === "private"
              ? t("phone.original.private")
              : load.why === "gone"
                ? t("phone.original.gone")
                : alone
                  ? t("phone.alone.original.failed")
                  : t("phone.original.failed")}
          </h1>
          {load.why === "failed" && (
            <div className="ph-spacer">
              <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
                {t("phone.tryAgain")}
              </Button>
            </div>
          )}
        </main>
      </Page>
    );
  }

  const { item } = load;
  const meta = t("phone.original.meta", {
    kind:
      item.kind === "audio" && item.durationMs !== null
        ? `${t(KIND.audio)} · ${formatDuration(item.durationMs)}`
        : t(KIND[item.kind]),
    day: formatDay(item.keptAt),
  });

  return (
    <Page bar={bar} fixed>
      <main
        className="ph-main ph-main--tight"
        style={{ padding: "18px 20px 20px", overflowY: "auto" }}
      >
        <div className="ph-stack" style={{ gap: 4 }}>
          <h1
            style={{ margin: 0, fontSize: "1.25rem", fontWeight: 500 }}
            tabIndex={-1}
            ref={heading}
          >
            {item.title}
          </h1>
          <span className="ph-rc">{meta}</span>
        </div>
        {item.kind === "image" && rendering === "image" && url && (
          <>
            <div className="ph-original-photo">
              <img src={url} alt={item.title} />
            </div>
            <p className="ph-hint ph-hint--ink2">
              {alone ? t("phone.alone.original.zoom") : t("phone.original.zoom")}
            </p>
          </>
        )}
        {item.kind === "audio" && rendering === "audio" && url && (
          <Voice url={url} item={item} alone={alone !== undefined} />
        )}
        {item.kind === "document" && rendering === "pdf" && url && (
          <>
            <Button kind="primary" onClick={() => openDocument(url, fileName(item.title, "pdf"))}>
              <Icon name="document" size={20} />
              {t("phone.original.document")}
            </Button>
            <a className="ph-btn ph-btn--quiet" href={url} download={fileName(item.title, "pdf")}>
              {alone ? t("phone.alone.original.download") : t("phone.original.download")}
            </a>
            <p className="ph-hint ph-hint--ink2">
              {alone ? t("phone.alone.original.document.hint") : t("phone.original.document.hint")}
            </p>
          </>
        )}
        {rendering === "download" && url && (
          <>
            <p className="ph-body">{t("phone.original.cantShowHere")}</p>
            <a
              className="ph-btn ph-btn--primary"
              href={url}
              download={fileName(item.title, "heic")}
            >
              {alone ? t("phone.alone.original.download") : t("phone.original.download")}
            </a>
          </>
        )}
        {blob && !rendering && (
          <p className="ph-hint ph-hint--ink2">
            {alone ? t("phone.alone.original.cantShow") : t("phone.original.cantShow")}
          </p>
        )}
        {(item.kind === "text" || item.kind === "web") && (
          <>
            <article className="ph-text-card">
              {marked(item.text ?? item.summary ?? "", quote)}
            </article>
            {item.truncated && (
              <p className="ph-hint ph-hint--ink2">
                {alone ? t("phone.alone.original.cut") : t("phone.original.cut")}
              </p>
            )}
            {quote && (item.text ?? "").includes(quote) && (!alone || alone.found) && (
              <p className="ph-hint ph-hint--ink2">
                {alone ? t("phone.alone.original.found") : t("phone.original.used")}
              </p>
            )}
            {alone ? null : (
              <>
                <div className="ph-spacer" />
                <p className="ph-hint ph-hint--ink2">{t("phone.original.text")}</p>
              </>
            )}
          </>
        )}
        {alone ? (
          <PrivateRow
            item={item}
            onChange={async (on) => {
              const next = await alone.setPrivate(item.id, on);
              setLoad((now) => (now.name === "ready" ? { ...now, item: next } : now));
            }}
          />
        ) : null}
      </main>
    </Page>
  );
}

function fileName(title: string, extension: string): string {
  const base =
    title
      .replace(/[^\p{L}\p{N} ._-]+/gu, "")
      .trim()
      .slice(0, 60) || "Nocturne";
  return `${base}.${extension}`;
}

/** Opens a document in the browser's viewer; if that is blocked or fails, saves it instead. */
export function openDocument(url: string, name: string): void {
  try {
    const opened = window.open(url, "_blank");
    if (opened) {
      opened.opener = null;
      return;
    }
  } catch {
    // Falls through to the download.
  }
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
}

function marked(text: string, quote: string | null) {
  if (!quote) return text;
  const at = text.indexOf(quote);
  if (at < 0) return text;
  return (
    <>
      {text.slice(0, at)}
      <mark>{quote}</mark>
      {text.slice(at + quote.length)}
    </>
  );
}

/** Marking a thing private, on the phone that holds it. Journal entries never can be. */
function PrivateRow({ item, onChange }: { item: PhoneItem; onChange(on: boolean): Promise<void> }) {
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  if (item.journal)
    return <p className="ph-hint ph-hint--ink2">{t("phone.alone.private.journal")}</p>;
  return (
    <>
      <Switch
        label={t("phone.alone.private")}
        hint={t("phone.alone.private.hint")}
        on={item.private === true}
        disabled={busy}
        onChange={(on) => {
          setBusy(true);
          setFailed(false);
          onChange(on).then(
            () => setBusy(false),
            () => {
              setBusy(false);
              setFailed(true);
            },
          );
        }}
      />
      {failed ? (
        <p className="ph-error" role="alert">
          {t("item.failed")}
        </p>
      ) : null}
    </>
  );
}

function Voice({ url, item, alone = false }: { url: string; item: PhoneItem; alone?: boolean }) {
  const audio = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [at, setAt] = useState(0);
  const [total, setTotal] = useState((item.durationMs ?? 0) / 1000);
  const heard = item.text ?? item.summary;
  return (
    <>
      <div className="ph-player">
        {/* biome-ignore lint/a11y/useMediaCaption: the transcript is shown beneath the player */}
        <audio
          ref={audio}
          src={url}
          preload="metadata"
          onLoadedMetadata={(e) =>
            Number.isFinite(e.currentTarget.duration) && setTotal(e.currentTarget.duration)
          }
          onTimeUpdate={(e) => setAt(e.currentTarget.currentTime)}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onEnded={() => setPlaying(false)}
        />
        <button
          type="button"
          className="ph-play"
          aria-label={playing ? t("phone.original.pause") : t("phone.original.play")}
          onClick={() => (playing ? audio.current?.pause() : void audio.current?.play())}
        >
          <Icon name={playing ? "pause" : "play"} size={26} />
        </button>
        <div className="ph-seek">
          <input
            type="range"
            min={0}
            max={Math.max(total, 0.1)}
            step={0.1}
            value={Math.min(at, Math.max(total, 0.1))}
            aria-label={t("phone.original.position")}
            aria-valuetext={t("phone.original.positionText", {
              at: formatDuration(at * 1000),
              total: formatDuration(total * 1000),
            })}
            onChange={(e) => {
              if (audio.current) audio.current.currentTime = Number(e.target.value);
              setAt(Number(e.target.value));
            }}
          />
          <div className="ph-times">
            <span className="ph-rc">{formatDuration(at * 1000)}</span>
            <span className="ph-rc">{formatDuration(total * 1000)}</span>
          </div>
        </div>
      </div>
      {heard && (
        <div className="ph-stack">
          <span className="ph-lbl">
            {alone && item.heardBy === "phone" ? t("phone.alone.heard") : t("phone.original.heard")}
          </span>
          <p className="ph-answer" style={{ fontSize: "1.25rem", lineHeight: 1.5 }}>
            “{heard}”
          </p>
        </div>
      )}
      {alone ? null : (
        <>
          <div className="ph-spacer" />
          <p className="ph-hint ph-hint--ink2">{t("phone.original.voice")}</p>
        </>
      )}
    </>
  );
}
