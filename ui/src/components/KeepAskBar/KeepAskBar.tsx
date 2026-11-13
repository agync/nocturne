import { type KeyboardEvent, type Ref, useId, useState } from "react";
import { t } from "../../copy/t";
import { Icon } from "../Icon/Icon";
import styles from "./KeepAskBar.module.css";

export type Verb = "keep" | "ask";

const QUESTION_WORDS = /^(who|what|when|where|why|how|is|are|do|does|did|can|will)\b/i;

/** A question mark or a question word makes Ask the default; anything else is a Keep. */
export function defaultVerb(text: string): Verb {
  const clean = text.trim();
  if (clean.endsWith("?") || QUESTION_WORDS.test(clean)) return "ask";
  return "keep";
}

/** "Ctrl+Shift+Space" -> "Ctrl Shift Space" */
export function shortcutLabel(shortcut: string): string {
  return shortcut.split("+").join(" ");
}

interface KeepAskBarProps {
  placement: "top" | "bottom";
  onKeep: (text: string) => void | Promise<void>;
  onAsk: (text: string) => void | Promise<void>;
  /** The global quick-keep shortcut, shown as a hint while the bar is empty. */
  shortcut?: string;
  /** Hold-to-talk. Press starts, release stops. */
  onTalk?: { start: () => void; stop: () => void };
  /** Why talking is not possible here. The microphone button is marked unavailable and says why. */
  talkUnavailable?: string;
  /** Seconds recorded so far, while the microphone is open. */
  recordingSeconds?: number | null;
  /** The page offers "/" to jump to the bar, so the bar says so to a screen reader. */
  focusShortcut?: boolean;
  inputRef?: Ref<HTMLTextAreaElement>;
}

/** One input, two verbs. Both are always visible once there is something to send. */
export function KeepAskBar({
  placement,
  onKeep,
  onAsk,
  shortcut,
  onTalk,
  recordingSeconds = null,
  talkUnavailable,
  focusShortcut = false,
  inputRef,
}: KeepAskBarProps) {
  const [text, setText] = useState("");
  const hintId = useId();
  const slashId = useId();
  const whyId = useId();
  const verb = defaultVerb(text);
  const hasText = text.trim().length > 0;
  const narrow = placement === "bottom";

  const send = async (as: Verb) => {
    const value = text.trim();
    if (!value) return;
    setText("");
    try {
      await (as === "keep" ? onKeep(value) : onAsk(value));
    } catch {
      // Nothing is lost: what was typed comes back unless the person has already started again.
      setText((current) => (current === "" ? value : current));
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing || event.shiftKey) return;
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) void send("keep");
    else if (event.altKey) void send("ask");
    else void send(verb);
  };

  const talk = onTalk
    ? {
        onPointerDown: (event: React.PointerEvent) => {
          event.preventDefault();
          onTalk.start();
        },
        onPointerUp: () => onTalk.stop(),
        onPointerLeave: () => {
          if (recordingSeconds !== null) onTalk.stop();
        },
        onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => {
          if (event.key === " " || event.key === "Enter") {
            event.preventDefault();
            if (!event.repeat && recordingSeconds === null) onTalk.start();
          }
        },
        onKeyUp: (event: KeyboardEvent<HTMLButtonElement>) => {
          if (event.key === " " || event.key === "Enter") {
            event.preventDefault();
            onTalk.stop();
          }
        },
      }
    : undefined;

  return (
    <search
      className={styles.bar}
      aria-label={t("shell.region.keep")}
      data-placement={placement}
      data-verb={verb}
    >
      <div className={styles.field}>
        <textarea
          ref={inputRef}
          className={styles.input}
          rows={1}
          value={text}
          aria-label={t("bar.label")}
          aria-describedby={focusShortcut ? `${hintId} ${slashId}` : hintId}
          aria-keyshortcuts={
            focusShortcut ? "Control+Enter Alt+Enter /" : "Control+Enter Alt+Enter"
          }
          placeholder={narrow ? t("bar.placeholder.narrow") : t("bar.placeholder")}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <span id={hintId} className="sr-only">
          {t(verb === "ask" ? "bar.default.ask" : "bar.default.keep")}
        </span>
        {focusShortcut ? (
          <span id={slashId} className="sr-only">
            {t("bar.hint.slash")}
          </span>
        ) : null}
        {hasText ? (
          <div className={styles.verbs}>
            <button
              type="button"
              className={styles.verb}
              data-variant={verb === "keep" ? "primary" : "secondary"}
              onClick={() => void send("keep")}
            >
              {t("bar.keep")}
              <kbd className={styles.key}>{verb === "keep" ? "↵" : "Ctrl ↵"}</kbd>
            </button>
            <button
              type="button"
              className={styles.verb}
              data-variant={verb === "ask" ? "primary" : "secondary"}
              onClick={() => void send("ask")}
            >
              {t("bar.ask")}
              <kbd className={styles.key}>{verb === "ask" ? "↵" : "Alt ↵"}</kbd>
            </button>
          </div>
        ) : recordingSeconds !== null ? (
          <span className={styles.listening} role="status">
            <span className={styles.rec} aria-hidden="true" />
            {t("bar.listening", { time: formatClock(recordingSeconds) })}
          </span>
        ) : shortcut && !narrow ? (
          <kbd
            className={styles.shortcut}
            aria-label={t("bar.shortcut.label", { keys: shortcutLabel(shortcut) })}
          >
            {shortcutLabel(shortcut)}
          </kbd>
        ) : null}
        {onTalk ? (
          <button
            type="button"
            className={styles.mic}
            data-recording={recordingSeconds !== null}
            aria-label={t("bar.talk")}
            aria-pressed={recordingSeconds !== null}
            aria-disabled={talkUnavailable ? true : undefined}
            aria-describedby={talkUnavailable ? whyId : undefined}
            title={talkUnavailable}
            {...talk}
          >
            <Icon name="mic" size={18} />
          </button>
        ) : null}
        {onTalk && talkUnavailable ? (
          <span id={whyId} className="sr-only">
            {talkUnavailable}
          </span>
        ) : null}
      </div>
    </search>
  );
}

function formatClock(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = String(seconds % 60).padStart(2, "0");
  return `${m}:${s}`;
}
