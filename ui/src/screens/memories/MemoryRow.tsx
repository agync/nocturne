import { useEffect, useId, useRef, useState } from "react";
import { useLocation } from "wouter";
import { Button } from "../../components/Button/Button";
import { Certainty } from "../../components/Certainty/Certainty";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { StatusLabel } from "../../components/StatusLabel/StatusLabel";
import { TextInput } from "../../components/TextInput/TextInput";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { MemoryView } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import styles from "./Memories.module.css";
import { memoryWhen } from "./memoryText";
import { MEMORY_KEYS } from "./queries";

interface MemoryRowProps {
  memory: MemoryView;
  selected: boolean;
  onSelect: (id: string | null) => void;
  /** Opens the forget dialog for this memory; the screen owns the dialog so it outlives the row. */
  onForget: (memory: MemoryView) => void;
  /** Opens "Is this about something else?"; the screen owns the dialog for the same reason. */
  onRefile: (memory: MemoryView) => void;
}

/**
 * One memory: its sentence, certainty, receipts and words for its state. The row is a button;
 * choosing it opens a bar of actions underneath.
 */
export function MemoryRow({ memory: m, selected, onSelect, onForget, onRefile }: MemoryRowProps) {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(m.sentence);
  const [busy, setBusy] = useState(false);
  const barId = useId();
  const button = useRef<HTMLButtonElement>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!selected) setEditing(false);
  }, [selected]);

  useEffect(() => {
    if (editing) field.current?.focus();
  }, [editing]);

  const run = async (work: () => Promise<unknown>, message: string): Promise<boolean> => {
    setBusy(true);
    try {
      await work();
      refresh(...MEMORY_KEYS);
      toast.show({ message });
      return true;
    } catch {
      toast.show({ message: t("memories.failed") });
      return false;
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    const text = draft.trim();
    if (text === "" || text === m.sentence) {
      setEditing(false);
      button.current?.focus();
      return;
    }
    // A failed save keeps what was typed, so it can be sent again.
    setBusy(true);
    try {
      const corrected = await engine.correctMemory(m.id, text);
      refresh(...MEMORY_KEYS);
      // The correction may mean the memory is filed under the wrong person, place or thing.
      toast.show({
        message: t("memory.correct.saved"),
        action: { label: t("memory.refile.offer"), onAction: () => onRefile(corrected) },
        duration: 12_000,
      });
    } catch {
      toast.show({ message: t("memories.failed") });
      setBusy(false);
      return;
    }
    setBusy(false);
    setEditing(false);
    button.current?.focus();
  };

  return (
    <li
      className={styles.memory}
      data-selected={selected}
      data-status={m.status}
      onKeyDown={(event) => {
        if (event.key === "Escape" && selected) {
          event.stopPropagation();
          onSelect(null);
          button.current?.focus();
        }
      }}
    >
      <button
        ref={button}
        type="button"
        className={styles.memoryButton}
        aria-expanded={selected}
        aria-controls={selected ? barId : undefined}
        onClick={() => onSelect(selected ? null : m.id)}
      >
        <span className={`t-page-line ${styles.sentence}`}>{m.sentence}</span>
        <span className={styles.side}>
          <Certainty level={m.certainty} />
          <span className={styles.when}>{memoryWhen(m)}</span>
        </span>
      </button>

      <div className={styles.tags}>
        {m.receipts.map((r) => (
          <ReceiptChip
            key={`${r.itemId}-${JSON.stringify(r.span)}`}
            receipt={r}
            onOpen={() => navigate(`/items/${r.itemId}`)}
          />
        ))}
        {m.was ? <span className={styles.changed}>{t("memory.flag.changed")}</span> : null}
        {m.movedFrom ? (
          <span className={styles.changed}>{t("memory.movedFrom", { name: m.movedFrom })}</span>
        ) : null}
        {m.status === "unconfirmed" ? <StatusLabel status="unconfirmed" /> : null}
        {m.status === "fading" ? <StatusLabel status="fading" /> : null}
        {m.status === "letGo" ? <StatusLabel status="letGo" /> : null}
        {m.private ? <StatusLabel status="private" /> : null}
        {m.keepForever ? <StatusLabel status="keepForever" /> : null}
      </div>

      {selected ? (
        <fieldset id={barId} className={styles.actions}>
          <legend className="sr-only">{t("memory.actions.label", { sentence: m.sentence })}</legend>
          {editing ? (
            <form
              className={styles.correct}
              onSubmit={(event) => {
                event.preventDefault();
                void save();
              }}
            >
              <TextInput
                label={t("memory.correct.label")}
                value={draft}
                inputRef={field}
                onChange={setDraft}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.stopPropagation();
                    setEditing(false);
                    button.current?.focus();
                  }
                }}
              />
              <div className={styles.buttons}>
                <Button type="submit" variant="primary" disabled={busy}>
                  {t("memory.correct.save")}
                </Button>
                <Button
                  variant="secondary"
                  onClick={() => {
                    setEditing(false);
                    button.current?.focus();
                  }}
                >
                  {t("dialog.cancel")}
                </Button>
              </div>
            </form>
          ) : (
            <div className={styles.buttons}>
              <Button
                variant="secondary"
                onClick={() => {
                  setDraft(m.sentence);
                  setEditing(true);
                }}
              >
                {t("memory.action.correct")}
              </Button>
              <Button
                variant="secondary"
                aria-pressed={m.keepForever}
                disabled={busy}
                onClick={() =>
                  void run(
                    () => engine.setKeepForever(m.id, !m.keepForever),
                    t(m.keepForever ? "memory.keepForever.off" : "memory.keepForever.on"),
                  )
                }
              >
                {t("memory.action.keepForever")}
              </Button>
              <Button
                variant="secondary"
                aria-pressed={m.private}
                disabled={busy}
                onClick={() =>
                  void run(
                    () => engine.setPrivate(m.id, !m.private),
                    t(m.private ? "memory.private.off" : "memory.private.on"),
                  )
                }
              >
                {t("memory.action.private")}
              </Button>
              <Button variant="secondary" onClick={() => onRefile(m)}>
                {t("memory.action.refile")}
              </Button>
              <Button variant="forget" onClick={() => onForget(m)}>
                {t("memory.action.forget")}
              </Button>
            </div>
          )}
          {m.suggestPrivate && !m.private && !editing ? (
            <p className={styles.note}>{t("memory.suggestPrivate")}</p>
          ) : null}
        </fieldset>
      ) : null}
    </li>
  );
}
