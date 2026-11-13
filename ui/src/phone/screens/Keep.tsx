import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { canCaptureSound, startWavCapture } from "../../app/wavCapture";
import { formatBytes, formatDuration, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { PhoneError, PhoneKept } from "../../engine/bindings";
import { copy } from "../bytes";
import { sniffMime } from "../crypto";
import {
  type Connection,
  ConnectionError,
  type PendingKeep,
  type PhoneEngine,
  PhoneRequestError,
  type Queue,
} from "../engine";
import { MAX_CAPTION, MAX_ITEM, MAX_TEXT } from "../protocol";
import { withKeepLock } from "../queue";
import { BackBar, Button, Icon, Page } from "./ui";

type Kind = "text" | "photo" | "recording";

type State =
  | { name: "compose" }
  | { name: "photo"; blob: Blob; url: string }
  | { name: "recording" }
  | { name: "sending" }
  | {
      name: "kept";
      kind: Kind;
      kept: PhoneKept;
      url: string | null;
      undone: boolean;
      late: boolean;
    }
  | { name: "waiting"; kind: Kind; title: string; at: number; url: string | null }
  | { name: "problem"; kind: "big" | "failed" | "kind" };

const KIND_LABEL: Record<
  Kind,
  "phone.keep.kind.text" | "phone.keep.kind.photo" | "phone.keep.kind.recording"
> = {
  text: "phone.keep.kind.text",
  photo: "phone.keep.kind.photo",
  recording: "phone.keep.kind.recording",
};

function clientId(): string {
  const c = globalThis.crypto;
  if (typeof c.randomUUID === "function") return c.randomUUID();
  return Array.from(c.getRandomValues(new Uint8Array(16)), (b) =>
    b.toString(16).padStart(2, "0"),
  ).join("");
}

const encoder = new TextEncoder();
const capped = (text: string, max: number) => [...text].slice(0, max).join("");

const WAIT_CODES: ReadonlySet<PhoneError> = new Set([
  "busy",
  "tooMany",
  "notReady",
  "paused",
  "sessionExpired",
]);

interface KeepProps {
  engine: PhoneEngine;
  queue: Queue;
  connection: Connection;
  deviceName: string;
  now(): number;
  onHome(): void;
  onQueued(): void;
  /** For a scripted build that has no microphone. */
  recorder?: RecorderFactory;
}

export interface Recording {
  /** Ends the recording: the sound as a WAV file, or null when none was heard. */
  stop(): Promise<Blob | null>;
  cancel(): void;
}
/** `onLimit` is called once if the longest recording a phone can keep has been reached. */
export type RecorderFactory = (onLimit: () => void) => Promise<Recording>;

/** 16 kHz mono 16-bit sound is 32 KB a second; this many seconds always fit the 50 MB limit. */
const MAX_RECORDING_SECONDS = Math.floor((MAX_ITEM - 44) / 32000);

/**
 * The phone's own recorder: the microphone through Web Audio into a 16 kHz mono 16-bit WAV, the
 * same as the computer's, because that is the one form the speech reader takes. The sound is held
 * in memory as 16-bit numbers until the recording ends.
 */
export const browserRecorder: RecorderFactory = async (onLimit) => {
  const media = globalThis.navigator?.mediaDevices;
  if (!media?.getUserMedia || !canCaptureSound()) throw new Error("none");
  const stream = await media.getUserMedia({ audio: true });
  const release = () => {
    for (const track of stream.getTracks()) track.stop();
  };
  let capture: ReturnType<typeof startWavCapture>;
  try {
    capture = startWavCapture(stream, onLimit, MAX_RECORDING_SECONDS);
  } catch {
    release();
    throw new Error("none");
  }
  return {
    async stop() {
      const file = capture.finish();
      release();
      return file ? new Blob([copy(file)], { type: "audio/wav" }) : null;
    },
    cancel() {
      capture.discard();
      release();
    },
  };
};

export function Keep({
  engine,
  queue,
  connection,
  deviceName,
  now,
  onHome,
  onQueued,
  recorder = browserRecorder,
}: KeepProps) {
  const [state, setState] = useState<State>({ name: "compose" });
  const [text, setText] = useState("");
  const [caption, setCaption] = useState("");
  const [micError, setMicError] = useState<"denied" | "none" | null>(null);
  const urls = useRef(new Set<string>());
  const take = useRef<HTMLInputElement>(null);
  const choose = useRef<HTMLInputElement>(null);
  const textArea = useRef<HTMLTextAreaElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  const url = useCallback((blob: Blob) => {
    const u = URL.createObjectURL(blob);
    urls.current.add(u);
    return u;
  }, []);
  const revoke = useCallback((u: string | null) => {
    if (!u) return;
    URL.revokeObjectURL(u);
    urls.current.delete(u);
  }, []);
  useEffect(
    () => () => {
      for (const u of urls.current) URL.revokeObjectURL(u);
    },
    [],
  );

  const key = state.name === "problem" ? `problem-${state.kind}` : state.name;
  useEffect(() => {
    void key;
    if (key !== "compose") heading.current?.focus();
  }, [key]);

  const connected = connection.kind === "connected";

  const submit = async (kind: Kind, body: { text?: string; blob?: Blob; caption?: string }) => {
    const size =
      body.text !== undefined ? encoder.encode(body.text).length : (body.blob?.size ?? 0);
    const limit = kind === "text" ? MAX_TEXT : MAX_ITEM;
    if (size > limit) return setState({ name: "problem", kind: "big" });
    if (body.blob) {
      const head = new Uint8Array(await body.blob.slice(0, 32).arrayBuffer());
      if (kind === "photo" && !sniffMime(head)) return setState({ name: "problem", kind: "kind" });
    }
    const keep: PendingKeep = {
      clientId: clientId(),
      kind,
      text: body.text,
      blob: body.blob,
      caption: body.caption,
      size,
      createdAt: now(),
    };
    const preview = kind === "photo" && body.blob ? url(body.blob) : null;
    // Written first, so a reload or a phone that kills the page loses nothing: the entry goes
    // when the computer has the keep, and the client id keeps a second send harmless.
    await queue.add(keep);
    onQueued();
    const wait = () =>
      setState({
        name: "waiting",
        kind,
        title: kind === "text" ? (body.text ?? "") : (body.caption ?? ""),
        at: keep.createdAt,
        url: preview,
      });
    const drop = async () => {
      await queue.remove(keep.clientId);
      onQueued();
    };
    if (!connected) return wait();
    setState({ name: "sending" });
    try {
      const kept = await withKeepLock(() =>
        kind === "text"
          ? engine.keepText(body.text ?? "", keep.clientId)
          : kind === "photo"
            ? engine.keepBytes(body.blob as Blob, keep.clientId, body.caption)
            : engine.keepRecording(body.blob as Blob, keep.clientId),
      );
      await drop();
      setState({ name: "kept", kind, kept, url: preview, undone: false, late: false });
      setText("");
      setCaption("");
    } catch (e) {
      if (e instanceof ConnectionError) return wait();
      if (e instanceof PhoneRequestError) {
        // Busy, rate limited or not ready: the keep goes back to waiting, to be sent again.
        if (WAIT_CODES.has(e.code)) return wait();
        await drop();
        if (e.code === "tooLarge") return setState({ name: "problem", kind: "big" });
        if (e.code === "invalid") return setState({ name: "problem", kind: "kind" });
        return setState({ name: "problem", kind: "failed" });
      }
      await drop();
      setState({ name: "problem", kind: "failed" });
    }
  };

  const another = () => {
    if (state.name === "kept" || state.name === "waiting") revoke(state.url);
    setState({ name: "compose" });
    setText("");
    setCaption("");
    setTimeout(() => textArea.current?.focus(), 0);
  };

  const pick = (file: File | undefined) => {
    if (!file) return;
    setCaption("");
    setState({ name: "photo", blob: file, url: url(file) });
  };

  const back = () => {
    if (state.name === "photo") {
      revoke(state.url);
      return setState({ name: "compose" });
    }
    onHome();
  };

  if (state.name === "recording") {
    return (
      <Recorder
        factory={recorder}
        onCancel={() => setState({ name: "compose" })}
        onDone={(blob) => {
          // No sound was heard: nothing to keep.
          if (!blob) return setState({ name: "compose" });
          void submit("recording", { blob });
        }}
        onError={(e) => {
          setMicError(e);
          setState({ name: "compose" });
        }}
      />
    );
  }

  const bar = (
    <BackBar
      title={state.name === "photo" ? t("phone.keep.title.photo") : t("phone.keep.title")}
      onBack={back}
    />
  );

  if (state.name === "compose") {
    return (
      <Page bar={bar}>
        <main className="ph-main ph-main--tight" style={{ gap: 16, padding: 20 }}>
          <form
            className="ph-stack"
            style={{ gap: 16, flex: 1 }}
            onSubmit={(e) => {
              e.preventDefault();
              if (text.trim() !== "") void submit("text", { text });
            }}
          >
            <label htmlFor="note" className="ph-label">
              {t("phone.keep.type")}
            </label>
            <textarea
              id="note"
              ref={textArea}
              className="ph-textarea"
              placeholder={t("phone.keep.placeholder")}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <span className="ph-hint" style={{ marginTop: -4 }}>
              {t("phone.keep.or")}
            </span>
            <div className="ph-sources">
              <button type="button" className="ph-source" onClick={() => take.current?.click()}>
                <Icon name="camera" />
                {t("phone.keep.take")}
              </button>
              <button type="button" className="ph-source" onClick={() => choose.current?.click()}>
                <Icon name="image" />
                {t("phone.keep.choose")}
              </button>
              <button
                type="button"
                className="ph-source"
                onClick={() => {
                  setMicError(null);
                  setState({ name: "recording" });
                }}
              >
                <Icon name="mic" />
                {t("phone.keep.record")}
              </button>
            </div>
            {micError && (
              <p className="ph-error" role="alert">
                {micError === "denied" ? t("phone.keep.micDenied") : t("phone.keep.micNone")}
              </p>
            )}
            <input
              ref={take}
              type="file"
              accept="image/*"
              capture="environment"
              hidden
              tabIndex={-1}
              aria-hidden="true"
              onChange={(e) => {
                pick(e.target.files?.[0]);
                e.target.value = "";
              }}
            />
            <input
              ref={choose}
              type="file"
              accept="image/*"
              hidden
              tabIndex={-1}
              aria-hidden="true"
              onChange={(e) => {
                pick(e.target.files?.[0]);
                e.target.value = "";
              }}
            />
            <div className="ph-spacer ph-stack" style={{ gap: 12 }}>
              <p className="ph-hint">{t("phone.keep.fromName", { name: deviceName })}</p>
              <Button kind="primary" type="submit" disabled={text.trim() === ""}>
                {t("phone.keep.submit")}
              </Button>
            </div>
          </form>
        </main>
      </Page>
    );
  }

  if (state.name === "photo") {
    return (
      <Page bar={bar}>
        <main className="ph-main ph-main--tight" style={{ padding: "16px 20px 20px" }}>
          <div className="ph-photo">
            <img src={state.url} alt={t("phone.keep.photo.alt")} />
          </div>
          <div className="ph-row-between">
            <span className="ph-rc">
              {t("phone.keep.photo.meta", { size: formatBytes(state.blob.size) })}
            </span>
            <button
              type="button"
              className="ph-ib ph-ib--accent"
              style={{ height: 44 }}
              onClick={() => {
                revoke(state.url);
                setState({ name: "compose" });
                setTimeout(() => take.current?.click(), 0);
              }}
            >
              {t("phone.keep.retake")}
            </button>
          </div>
          <div className="ph-field" style={{ gap: 6 }}>
            <label htmlFor="cap">
              {t("phone.keep.note")}{" "}
              <span className="ph-label-opt">{t("phone.keep.note.optional")}</span>
            </label>
            <input
              id="cap"
              className="ph-input ph-input--note"
              value={caption}
              autoComplete="off"
              onChange={(e) => setCaption(capped(e.target.value, MAX_CAPTION))}
            />
          </div>
          <div className="ph-spacer" />
          <Button
            kind="primary"
            onClick={() =>
              void submit("photo", { blob: state.blob, caption: caption.trim() || undefined })
            }
          >
            {t("phone.keep.submit")}
          </Button>
        </main>
      </Page>
    );
  }

  if (state.name === "sending") {
    return (
      <Page bar={bar}>
        <main className="ph-main" aria-busy="true">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {t("phone.keep.sending")}
          </p>
        </main>
      </Page>
    );
  }

  if (state.name === "problem") {
    const copy = {
      big: ["phone.keep.big.title", "phone.keep.big.body"],
      failed: ["phone.keep.failed.title", "phone.keep.failed.body"],
      kind: ["phone.keep.kind.title", "phone.keep.kind.body"],
    } as const;
    const [titleId, bodyId] = copy[state.kind];
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <div className="ph-live" role="alert" style={{ flex: "none" }}>
            <h1 className="ph-h1" tabIndex={-1} ref={heading}>
              {t(titleId)}
            </h1>
            <p className="ph-body">{t(bodyId)}</p>
          </div>
          <div className="ph-spacer ph-stack">
            <Button kind="primary" onClick={() => setState({ name: "compose" })}>
              {state.kind === "failed" ? t("phone.tryAgain") : t("phone.keep.another")}
            </Button>
            <Button kind="quiet" onClick={onHome}>
              {t("phone.home")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  if (state.name === "waiting") {
    return (
      <Page bar={bar}>
        <main className="ph-main" style={{ paddingTop: 40, gap: 20 }}>
          <div className="ph-kept" role="status">
            <span
              className="ph-icon-box ph-icon-box--round"
              style={{ width: 44, height: 44 }}
              aria-hidden="true"
            >
              <Icon name="moon" size={20} />
            </span>
            <h1 className="ph-kept-word" tabIndex={-1} ref={heading}>
              {t("phone.keep.waiting")}
            </h1>
          </div>
          <p className="ph-body">{t("phone.keep.waiting.body")}</p>
          <KeptCard
            kind={state.kind}
            title={state.title || t(KIND_LABEL[state.kind])}
            line={t("phone.keep.waiting.from", {
              kind: t(KIND_LABEL[state.kind]),
              time: formatTime(new Date(state.at)),
            })}
            url={state.url}
          />
          <div className="ph-spacer ph-stack">
            <Button kind="primary" onClick={another}>
              {t("phone.keep.another")}
            </Button>
            <Button kind="quiet" onClick={onHome}>
              {t("phone.home")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  const kept = state;
  return (
    <Page bar={bar}>
      <main className="ph-main" style={{ paddingTop: 40, gap: 20 }}>
        <div className="ph-kept" role="status">
          <span
            className="ph-icon-box ph-icon-box--round"
            style={{ width: 44, height: 44 }}
            aria-hidden="true"
          >
            <Icon name="check" size={22} />
          </span>
          <h1 className="ph-kept-word" tabIndex={-1} ref={heading}>
            {t("phone.keep.kept")}
          </h1>
        </div>
        <p className="ph-body">{t("phone.keep.kept.body")}</p>
        <KeptCard
          kind={kept.kind}
          title={kept.kept.item.title}
          line={t("phone.keep.kept.from", {
            kind: t(KIND_LABEL[kept.kind]),
            time: formatTime(kept.kept.item.keptAt),
            name: deviceName,
          })}
          url={kept.url}
        />
        <div className="ph-row-between">
          {kept.undone ? (
            <span className="ph-rc" role="status">
              {t("phone.keep.undone")}
            </span>
          ) : kept.late ? (
            <span className="ph-rc" role="status">
              {t("phone.keep.undo.late")}
            </span>
          ) : (
            <>
              <span className="ph-rc">
                {t("phone.keep.undo", { time: formatTime(kept.kept.takeBackUntil) })}
              </span>
              <button
                type="button"
                className="ph-ib ph-ib--accent"
                style={{ padding: "0 4px" }}
                onClick={async () => {
                  try {
                    await engine.unkeep(kept.kept.item.id);
                    setState({ ...kept, undone: true });
                  } catch (e) {
                    if (e instanceof PhoneRequestError && e.code === "tooLate")
                      setState({ ...kept, late: true });
                    else if (e instanceof PhoneRequestError || e instanceof ConnectionError)
                      setState({ ...kept, late: true });
                    else throw e;
                  }
                }}
              >
                {t("phone.keep.undo.button")}
              </button>
            </>
          )}
        </div>
        <div className="ph-spacer ph-stack">
          <Button kind="primary" onClick={another}>
            {t("phone.keep.another")}
          </Button>
          <Button kind="quiet" onClick={onHome}>
            {t("phone.home")}
          </Button>
        </div>
      </main>
    </Page>
  );
}

function KeptCard({
  kind,
  title,
  line,
  url,
}: {
  kind: Kind;
  title: string;
  line: string;
  url: string | null;
}) {
  return (
    <div className="ph-kept-card">
      <span className="ph-kept-thumb" aria-hidden="true">
        {url ? (
          <img src={url} alt="" />
        ) : (
          <Icon name={kind === "recording" ? "mic" : "page"} size={22} />
        )}
      </span>
      <span className="ph-stack" style={{ gap: 4 }}>
        <p className="ph-kept-title">{title}</p>
        <span className="ph-rc" style={{ color: "var(--ink2)" }}>
          {line}
        </span>
      </span>
    </div>
  );
}

function Recorder({
  factory,
  onCancel,
  onDone,
  onError,
}: {
  factory: RecorderFactory;
  onCancel(): void;
  onDone(blob: Blob | null): void;
  onError(kind: "denied" | "none"): void;
}) {
  const [seconds, setSeconds] = useState(0);
  const [live, setLive] = useState(false);
  const recording = useRef<Recording | null>(null);
  const errorRef = useRef(onError);
  errorRef.current = onError;
  const finishRef = useRef<() => Promise<void>>(async () => undefined);
  finishRef.current = async () => {
    const r = recording.current;
    if (!r) return;
    recording.current = null;
    onDone(await r.stop());
  };
  const bars = useMemo(() => Array.from({ length: 36 }, (_, i) => 8 + ((i * 37) % 56)), []);

  useEffect(() => {
    let gone = false;
    factory(() => void finishRef.current()).then(
      (r) => {
        if (gone) return r.cancel();
        recording.current = r;
        setLive(true);
      },
      (e: unknown) => {
        if (!gone) errorRef.current(e instanceof Error && e.message === "none" ? "none" : "denied");
      },
    );
    return () => {
      gone = true;
      recording.current?.cancel();
    };
  }, [factory]);

  useEffect(() => {
    if (!live) return;
    const timer = setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => clearInterval(timer);
  }, [live]);

  const time = formatDuration(seconds * 1000);
  return (
    <Page bar={<BackBar title={t("phone.keep.title.recording")} onBack={onCancel} />}>
      <main className="ph-main ph-record" style={{ gap: 22 }}>
        <span className="ph-lbl ph-rec-status" role="status">
          <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
          {t("phone.record.status")}
        </span>
        <p className="ph-timer" role="timer" aria-label={time}>
          {time}
        </p>
        <div className="ph-wave" aria-hidden="true">
          {bars.map((h, i) => (
            <span key={`${i}-${h}`} style={{ height: h, animationDelay: `${(i % 9) * 0.12}s` }} />
          ))}
        </div>
        <p className="ph-body" style={{ maxWidth: 300 }}>
          {t("phone.record.hint")}
        </p>
        <div className="ph-spacer ph-stack" style={{ alignItems: "center", gap: 14 }}>
          <button
            type="button"
            className="ph-stop"
            aria-label={t("phone.record.stop")}
            disabled={!live}
            onClick={() => void finishRef.current()}
          >
            <span />
          </button>
          <span style={{ fontSize: "0.9375rem", fontWeight: 500 }}>{t("phone.record.stop")}</span>
          <button type="button" className="ph-ib" style={{ marginTop: 6 }} onClick={onCancel}>
            {t("phone.cancel")}
          </button>
        </div>
      </main>
    </Page>
  );
}
