import { type KeyboardEvent, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
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
}

/** Shows the shortcut and lets the person press a new one. Tab and Esc always get out. */
export function ShortcutField({ value, available, onChange }: Props) {
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
            placeholder={t("settings.shortcut.placeholder")}
            onKeyDown={onKeyDown}
            onBlur={() => leave(false)}
          />
        ) : (
          <>
            <kbd className={styles.kbd}>{value}</kbd>
            <Button
              variant="quiet"
              aria-label={t("settings.shortcut.change.named")}
              onClick={() => setCapturing(true)}
            >
              {t("settings.shortcut.change")}
            </Button>
          </>
        )}
      </div>
      {capturing ? <span className={styles.note}>{t("settings.shortcut.cancel")}</span> : null}
      {note ? (
        <span className={styles.problem} role="alert">
          {note}
        </span>
      ) : null}
      {!available && !capturing ? (
        <span className={styles.problem}>{t("settings.shortcut.taken")}</span>
      ) : null}
    </div>
  );
}
