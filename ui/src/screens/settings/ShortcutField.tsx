import { useQuery } from "@tanstack/react-query";
import { type KeyboardEvent, useId, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import { shortcutKeys } from "../../platform/shortcutKeys";
import styles from "./Settings.module.css";

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph"]);

const NAMES: Record<string, string> = {
  " ": "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
};

/**
 * "Ctrl+Shift+Space" for a key press, "modifier" when it lacks Ctrl, Alt or Super, or null for a
 * modifier on its own (which waits for the key that goes with it).
 */
export function comboFromEvent(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): string | "modifier" | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return "modifier";
  const key = NAMES[event.key] ?? (event.key.length === 1 ? event.key.toUpperCase() : event.key);
  return [
    event.ctrlKey ? "Ctrl" : null,
    event.altKey ? "Alt" : null,
    event.shiftKey ? "Shift" : null,
    event.metaKey ? "Super" : null,
    key,
  ]
    .filter(Boolean)
    .join("+");
}

interface Props {
  value: string;
  available: boolean;
  onChange: (combo: string) => void;
  /** The id of whatever explains this row, so the control is read with it. */
  describedBy?: string;
}

/** Shows the shortcut and lets the person press a new one. Tab and Esc always get out. */
export function ShortcutField({ value, available, onChange, describedBy }: Props) {
  const engine = useEngine();
  const cancelId = useId();
  const noteId = useId();
  const problemId = useId();
  // Where keys cannot be shared with every app, a desktop shortcut that runs this does the job.
  const command = useQuery({
    queryKey: ["quickKeepCommand"],
    queryFn: () => engine.quickKeepCommand(),
    enabled: !available,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const [capturing, setCapturing] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const change = useRef<HTMLDivElement>(null);

  const leave = (focusButton: boolean) => {
    setCapturing(false);
    setNote(null);
    if (focusButton) setTimeout(() => change.current?.querySelector("button")?.focus(), 0);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Tab") {
      leave(false);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      leave(true);
      return;
    }
    event.preventDefault();
    const combo = comboFromEvent(event.nativeEvent);
    if (combo === null) return;
    if (combo === "modifier") {
      setNote(t("settings.shortcut.needsModifier"));
      return;
    }
    onChange(combo);
    leave(true);
  };

  return (
    <div className={styles.shortcut}>
      <div className={styles.shortcutLine} ref={change}>
        {capturing ? (
          <input
            readOnly
            // biome-ignore lint/a11y/noAutofocus: the person just pressed Change; the field must hear the keys
            autoFocus
            className={styles.capture}
            aria-label={t("settings.shortcut.capture")}
            aria-describedby={[describedBy, cancelId, note ? noteId : null]
              .filter(Boolean)
              .join(" ")}
            aria-invalid={note ? true : undefined}
            placeholder={t("settings.shortcut.placeholder")}
            onKeyDown={onKeyDown}
            onBlur={() => leave(false)}
          />
        ) : (
          <>
            <kbd className={styles.kbd}>{shortcutKeys(value).join("+")}</kbd>
            <Button
              variant="quiet"
              aria-label={t("settings.shortcut.change.named")}
              aria-describedby={
                [describedBy, !available ? problemId : null].filter(Boolean).join(" ") || undefined
              }
              onClick={() => setCapturing(true)}
            >
              {t("settings.shortcut.change")}
            </Button>
          </>
        )}
      </div>
      {capturing ? (
        <span id={cancelId} className={styles.note}>
          {t("settings.shortcut.cancel")}
        </span>
      ) : null}
      {note ? (
        <span id={noteId} className={styles.problem} role="alert">
          {note}
        </span>
      ) : null}
      {!available && !capturing ? (
        <span id={problemId} className={styles.problem}>
          {t("settings.shortcut.taken")}
          {command.data ? (
            <>
              {" "}
              {t("settings.shortcut.command")}{" "}
              <code className={styles.command}>{command.data}</code>
            </>
          ) : null}
        </span>
      ) : null}
    </div>
  );
}
