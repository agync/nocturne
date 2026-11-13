import {
  type ClipboardEvent,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { askRequests } from "../../app/ask-request";
import { goTo } from "../../app/query";
import type { RecorderFailure } from "../../app/useRecorder";
import { Icon } from "../../components/Icon/Icon";
import { defaultVerb } from "../../components/KeepAskBar/KeepAskBar";
import { Notices } from "../../components/Notices/Notices";
import { Thumb } from "../../components/Thumb/Thumb";
import { formatDay, formatDuration, formatDurationSpoken } from "../../copy/format";
import { t } from "../../copy/t";
import type { ItemKind, ItemView, Kept, Notice, Source } from "../../engine/bindings";
import { useEngine, useStatus } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import styles from "./Capture.module.css";
import { MIN_HOLD_MS, useQuickRecorder } from "./useQuickRecorder";

/** How long the "Kept." pill stays before the window closes by itself. */
export const CLOSE_AFTER_MS = 1600;

type View =
  | { name: "compose" }
  | { name: "kept"; message: string; itemIds: string[] }
  | { name: "duplicate"; item: ItemView };

const MIC_DENIED: Notice = {
  id: "mic-denied",
  code: "micDenied",
  itemId: null,
  etaSeconds: null,
  bytes: null,
};

const spokenKind = (kind: ItemKind) => t(`kind.${kind}.spoken`);

const kindOfMime = (mime: string): ItemKind =>
  mime.startsWith("image/") ? "image" : mime.startsWith("audio/") ? "audio" : "document";

export { MIN_HOLD_MS };

/** The floating window from the shortcut: type, paste, drop or talk, and it is kept. */
export default function Capture() {
  const engine = useEngine();
  const platform = usePlatform();
  const [text, setText] = useState("");
  const [fromWeb, setFromWeb] = useState(false);
  const [view, setView] = useState<View>({ name: "compose" });
  const [announcement, setAnnouncement] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [micDenied, setMicDenied] = useState(false);
  const [dropping, setDropping] = useState(false);
  const [pausedClose, setPausedClose] = useState(false);
  const { data: status } = useStatus();
  const input = useRef<HTMLTextAreaElement>(null);
  const page = useRef<HTMLElement>(null);
  const heldBySpace = useRef(false);
  const pressedAt = useRef(0);
  const working = useRef(false);
  const undoing = useRef(false);

  useEffect(() => {
    document.title = `${t("screen.capture.title")} · ${t("app.name")}`;
  }, []);

  const close = useCallback(async () => {
    await platform.closeQuickCapture();
    // In a browser the window is the main one: leaving it goes to Today, so follow the request.
    return window.location.hash.startsWith("#/quick-capture");
  }, [platform]);

  const finishKept = useCallback((kept: Kept[], message: string, spoken: string) => {
    const first = kept[0];
    if (!first) return;
    // Only what is new can be taken back: a duplicate is an older thing that must stay.
    const itemIds = kept.flatMap((k) => (k.type === "new" ? [k.item.id] : []));
    if (itemIds.length === 0) {
      setView({ name: "duplicate", item: first.item });
      return;
    }
    setView({ name: "kept", message, itemIds });
    setAnnouncement(spoken);
  }, []);

  const failed = useCallback(() => setError(t("bar.kept.error")), []);

  const onFailed = useCallback((reason: RecorderFailure) => {
    if (reason === "denied") setMicDenied(true);
    else setError(t(reason === "unsupported" ? "capture.mic.unsupported" : "capture.mic.failed"));
  }, []);

  const recorder = useQuickRecorder({
    onFinished: (bytes, mime, durationMs) => {
      engine
        .keepRecording(bytes, mime)
        .then((kept) =>
          finishKept(
            [kept],
            t("capture.kept.voice", { duration: formatDuration(durationMs) }),
            t("capture.announce.keptVoice", { duration: formatDurationSpoken(durationMs) }),
          ),
        )
        .catch(failed);
    },
    onFailed,
  });

  // A pill that was replaced never sent its mouse-leave, so nothing carries over to the next view.
  useEffect(() => {
    setPausedClose(false);
    // The pill replaces the field, so focus waits on the window itself, where Escape and Ctrl Z work.
    if (view.name === "kept") page.current?.focus({ preventScroll: true });
  }, [view]);

  // Close by itself, unless the person is reading the pill or has a hand on it.
  useEffect(() => {
    if (view.name !== "kept" || pausedClose) return;
    const timer = setTimeout(() => void close(), CLOSE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [view, pausedClose, close]);

  useEffect(
    () =>
      platform.onFileDrop(
        (paths) => {
          setDropping(false);
          if (working.current) return;
          working.current = true;
          engine
            .keepFiles(paths, "drop")
            .then((kept) => finishKept(kept, t("capture.kept.later"), t("capture.kept.lead")))
            .catch(failed)
            .finally(() => {
              working.current = false;
            });
        },
        (count) => setDropping(count !== null),
      ),
    [platform, engine, finishKept, failed],
  );

  const keep = useCallback(
    async (value: string) => {
      if (working.current) return;
      working.current = true;
      const source: Source = fromWeb ? "pastedWeb" : "quickCapture";
      try {
        const kept = await engine.keepText(value, source);
        setText("");
        setFromWeb(false);
        finishKept(
          [kept],
          t(fromWeb ? "capture.kept.web" : "capture.kept.later"),
          t("capture.kept.text"),
        );
      } catch {
        failed();
      } finally {
        working.current = false;
      }
    },
    [engine, fromWeb, finishKept, failed],
  );

  const ask = useCallback(
    async (value: string) => {
      if (working.current) return;
      working.current = true;
      try {
        askRequests.request(value);
        await platform.handOffAsk(value);
        if (!(await close())) goTo("/ask");
      } catch {
        failed();
      } finally {
        working.current = false;
      }
    },
    [platform, close, failed],
  );

  const send = (as: "keep" | "ask") => {
    const value = text.trim();
    if (!value) return;
    setError(null);
    void (as === "keep" ? keep(value) : ask(value));
  };

  const keepBytes = async (files: File[]) => {
    if (working.current) return;
    working.current = true;
    try {
      const kept: Kept[] = [];
      for (const file of files) {
        const bytes = new Uint8Array(await file.arrayBuffer());
        kept.push(await engine.keepBytes(bytes, file.type, "paste"));
      }
      const kind = kindOfMime(files[0]?.type ?? "");
      finishKept(
        kept,
        files.length > 1
          ? t("capture.files", { count: files.length })
          : kind === "image"
            ? t("capture.kept.photo")
            : t("capture.kept.later"),
        t("capture.kept.lead"),
      );
    } catch {
      failed();
    } finally {
      working.current = false;
    }
  };

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = [...event.clipboardData.files];
    if (files.length > 0) {
      event.preventDefault();
      void keepBytes(files);
      return;
    }
    // Something copied from a web page arrives with markup as well as words.
    if (event.clipboardData.types.includes("text/html")) setFromWeb(true);
  };

  const stopRecording = (keepIt: boolean) => {
    heldBySpace.current = false;
    recorder.stop(keepIt);
    if (!keepIt) setAnnouncement(t("capture.announce.discarded"));
  };

  const startRecording = (bySpace: boolean) => {
    setError(null);
    setMicDenied(false);
    heldBySpace.current = bySpace;
    pressedAt.current = Date.now();
    setAnnouncement(t("capture.announce.recording"));
    recorder.start();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (recorder.recording) {
      if (event.key === "Enter") {
        event.preventDefault();
        stopRecording(true);
      } else if (event.key === " ") {
        event.preventDefault();
      }
      return;
    }
    if (event.key === " " && text === "" && !event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault();
      if (!event.repeat) startRecording(true);
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    // Holding Enter must not keep the same thing again and again.
    if (event.repeat) return;
    if (event.ctrlKey || event.metaKey) send("keep");
    else if (event.altKey) send("ask");
    else send(defaultVerb(text));
  };

  const onKeyUp = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== " " || !heldBySpace.current) return;
    // The microphone may still be opening: the recorder remembers the release and stops at once.
    // A press this short was an accident, so nothing is kept.
    const long = Date.now() - pressedAt.current >= MIN_HOLD_MS;
    stopRecording(long);
  };

  /** Escape leaves from every view; Ctrl Z takes back what the pill says was kept. */
  const onPageKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      if (recorder.recording || recorder.starting()) stopRecording(false);
      else void close();
    } else if (
      view.name === "kept" &&
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === "z"
    ) {
      event.preventDefault();
      void undo();
    }
  };

  // Only the microphone matters in this window; every other notice belongs to the main one.
  const micNotices = [
    ...(status?.notices ?? []).filter((n) => n.code === "micDenied"),
    ...(micDenied ? [MIC_DENIED] : []),
  ].slice(0, 1);
  const verb = defaultVerb(text);
  const hasText = text.trim().length > 0;

  const undo = async () => {
    if (view.name !== "kept" || view.itemIds.length === 0 || undoing.current) return;
    undoing.current = true;
    try {
      const ids = view.itemIds;
      const results = await Promise.allSettled(ids.map((id) => engine.unkeep(id)));
      if (results.every((r) => r.status === "fulfilled")) {
        setAnnouncement(t("capture.undone"));
        setView({ name: "compose" });
        setTimeout(() => input.current?.focus(), 0);
        return;
      }
      // Say only what is true: ask which of the failed ones are still there.
      const failedIds = ids.filter((_, i) => results[i]?.status === "rejected");
      const stillThere = (
        await Promise.all(
          failedIds.map((id) =>
            engine.getItem(id).then(
              () => [id],
              () => [] as string[],
            ),
          ),
        )
      ).flat();
      const back = ids.length - stillThere.length;
      if (stillThere.length === 0) {
        setAnnouncement(t("capture.undone"));
        setView({ name: "compose" });
        setTimeout(() => input.current?.focus(), 0);
      } else if (back === 0) {
        failed();
      } else {
        setView((current) =>
          current.name === "kept" ? { ...current, itemIds: stillThere } : current,
        );
        setAnnouncement(
          t("capture.undone.some", { back, total: ids.length, rest: stillThere.length }),
        );
      }
    } catch {
      failed();
    } finally {
      undoing.current = false;
    }
  };

  const openItem = async (id: string) => {
    await platform.openInMain(`/items/${id}`);
    // A browser has one window: opening the item already left this screen.
    if (window.location.hash.startsWith("#/quick-capture")) await platform.closeQuickCapture();
  };

  return (
    <main
      ref={page}
      className={styles.page}
      tabIndex={-1}
      aria-label={t("capture.region")}
      onKeyDown={onPageKeyDown}
    >
      <h1 className="sr-only">{t("screen.capture.title")}</h1>
      <div className={styles.stack}>
        <div className={styles.live} role="status" aria-live="polite" aria-atomic="true">
          {announcement}
        </div>

        {view.name === "kept" ? (
          // biome-ignore lint/a11y/noStaticElementInteractions: hover and focus only hold the window open
          <div
            className={styles.pill}
            data-view="kept"
            onMouseEnter={() => setPausedClose(true)}
            onMouseLeave={() => setPausedClose(false)}
          >
            <span className={styles.pillMark} aria-hidden="true" />
            <strong className={styles.pillLead}>{t("capture.kept.lead")}</strong>
            <span className={styles.pillMessage}>{view.message}</span>
            {view.itemIds.length > 0 ? (
              <button
                type="button"
                className={styles.pillUndo}
                aria-keyshortcuts="Control+Z"
                onFocus={() => setPausedClose(true)}
                onBlur={() => setPausedClose(false)}
                onClick={() => void undo()}
              >
                {t("capture.undo")}
              </button>
            ) : null}
          </div>
        ) : view.name === "duplicate" ? (
          <div className={styles.duplicate} data-view="duplicate">
            <Thumb kind={view.item.kind} size={28} />
            <p className={styles.duplicateText}>
              {t("capture.duplicate", {
                kind: spokenKind(view.item.kind),
                day: formatDay(view.item.keptAt),
              })}
            </p>
            <button
              type="button"
              className={styles.link}
              // biome-ignore lint/a11y/noAutofocus: the only control in this view
              autoFocus
              onClick={() => void openItem(view.item.id)}
            >
              {t("capture.duplicate.open")}
            </button>
            <button type="button" className={styles.link} onClick={() => void close()}>
              {t("button.close")}
            </button>
          </div>
        ) : (
          <div className={styles.surface} data-recording={recorder.recording} data-drop={dropping}>
            {recorder.recording ? (
              <div className={styles.listening}>
                <span className={styles.rec} aria-hidden="true" />
                <span className={styles.listeningLabel}>{t("capture.listening")}</span>
                <span className={styles.meter} aria-hidden="true">
                  {recorder.levels.map((level, i) => (
                    // The bars are a rolling window of the last few moments; the index is the slot.
                    <span key={i} style={{ height: `${Math.round(level * 100)}%` }} />
                  ))}
                </span>
                <span className={styles.time} aria-hidden="true">
                  {formatDuration(recorder.elapsedMs)}
                </span>
              </div>
            ) : null}
            <div className={styles.entry}>
              <span className={styles.mark} aria-hidden="true" />
              <textarea
                ref={input}
                className={styles.input}
                data-hidden={recorder.recording}
                rows={1}
                value={text}
                readOnly={recorder.recording}
                aria-label={t("bar.label")}
                aria-keyshortcuts="Control+Enter Alt+Enter"
                placeholder={t("bar.placeholder")}
                // biome-ignore lint/a11y/noAutofocus: the window opens ready to type, by design
                autoFocus
                onChange={(event) => {
                  setText(event.target.value);
                  if (event.target.value.trim() === "") setFromWeb(false);
                }}
                onKeyDown={onKeyDown}
                onKeyUp={onKeyUp}
                onPaste={onPaste}
              />
              <button
                type="button"
                className={styles.mic}
                hidden={recorder.recording}
                aria-label={t("capture.mic.start")}
                onClick={() => startRecording(false)}
              >
                <Icon name="mic" size={16} />
              </button>
            </div>
            {fromWeb && hasText && !recorder.recording ? (
              <span className={styles.webChip}>
                <span className={styles.webDot} aria-hidden="true" />
                {t("capture.web.chip")}
              </span>
            ) : null}
            {error ? (
              <p className={styles.error} role="alert">
                {error}
              </p>
            ) : null}
            <div className={styles.footer}>
              {recorder.recording ? (
                <>
                  <span className={styles.hint}>
                    {t(
                      heldBySpace.current
                        ? "capture.recording.hint"
                        : "capture.recording.hint.click",
                    )}
                  </span>
                  <span className={styles.actions}>
                    <button
                      type="button"
                      className={styles.action}
                      onClick={() => stopRecording(false)}
                    >
                      {t("capture.discard")}
                    </button>
                    <button
                      type="button"
                      className={styles.action}
                      data-primary="true"
                      onClick={() => stopRecording(true)}
                    >
                      {t("bar.keep")}
                    </button>
                  </span>
                </>
              ) : hasText ? (
                <>
                  <span className={styles.hint}>
                    {fromWeb
                      ? t("capture.web.note")
                      : t(verb === "ask" ? "bar.default.ask" : "bar.default.keep")}
                  </span>
                  <span className={styles.actions}>
                    <button
                      type="button"
                      className={styles.action}
                      data-primary={verb === "ask"}
                      onClick={() => send("ask")}
                    >
                      {t("bar.ask")}
                      {verb === "ask" ? <kbd className={styles.key}>↵</kbd> : null}
                    </button>
                    <button
                      type="button"
                      className={styles.action}
                      data-primary={verb === "keep"}
                      onClick={() => send("keep")}
                    >
                      {t("bar.keep")}
                      {verb === "keep" ? <kbd className={styles.key}>↵</kbd> : null}
                    </button>
                  </span>
                </>
              ) : (
                <>
                  <span className={styles.hint}>
                    {t("capture.hint.before")}{" "}
                    <kbd className={styles.space}>{t("capture.hint.key")}</kbd>{" "}
                    {t("capture.hint.after")}
                  </span>
                  <span className={styles.esc}>{t("capture.esc")}</span>
                </>
              )}
            </div>
          </div>
        )}
        {micNotices.length > 0 ? (
          <Notices
            notices={micNotices}
            handlers={{
              micDenied: () => {
                engine.openSoundSettings().catch(() => undefined);
              },
            }}
          />
        ) : null}
      </div>
    </main>
  );
}
