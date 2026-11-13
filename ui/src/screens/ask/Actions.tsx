import { useQueryClient } from "@tanstack/react-query";
import { type RefObject, useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { t } from "../../copy/t";
import type { MemoryView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Ask.module.css";

type Action = "wrong" | "keepForever" | "forget";

interface ActionsProps {
  memories: MemoryView[];
  /** Said once, politely, after something was done. */
  result: string;
  onResult: (message: string) => void;
  /** The person gave the right sentence for a memory the answer used. */
  onCorrected: (sentence: string) => void;
  /** These memories were forgotten: the answer that used them no longer stands. */
  onForgotten: (ids: string[]) => void;
  /** After forgetting there is nothing left to do but read the result. */
  showButtons: boolean;
  resultRef: RefObject<HTMLParagraphElement | null>;
}

/**
 * That's wrong, Keep forever, Forget this. With several memories behind the answer each opens a
 * small chooser first, so the person always says which one.
 */
export function Actions({
  memories,
  showButtons,
  result,
  onResult,
  onCorrected,
  onForgotten,
  resultRef,
}: ActionsProps) {
  const engine = useEngine();
  const client = useQueryClient();
  const [chooser, setChooser] = useState<Action | null>(null);
  const [correcting, setCorrecting] = useState<MemoryView | null>(null);
  const [draft, setDraft] = useState("");
  const [forgetting, setForgetting] = useState<string[] | null>(null);
  const forgotten = useRef(false);
  const [busy, setBusy] = useState(false);
  const opener = useRef<HTMLButtonElement | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const firstChoice = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (correcting) field.current?.focus();
  }, [correcting]);
  useEffect(() => {
    if (chooser) firstChoice.current?.focus();
  }, [chooser]);

  const refresh = () => client.invalidateQueries({ queryKey: ["askMemories"] });
  const fail = () => onResult(t("ask.action.failed"));
  const allKeptForever = memories.length > 0 && memories.every((m) => m.keepForever);

  const run = async (action: Action, chosen: MemoryView[]) => {
    setChooser(null);
    if (action === "wrong") {
      const first = chosen[0];
      if (!first) return;
      setDraft(first.sentence);
      setCorrecting(first);
    } else if (action === "forget") {
      setForgetting(chosen.map((m) => m.id));
    } else {
      setBusy(true);
      try {
        await Promise.all(chosen.map((m) => engine.setKeepForever(m.id, true)));
        await refresh();
        onResult(t("ask.keepForever.done"));
        resultRef.current?.focus();
      } catch {
        fail();
      } finally {
        setBusy(false);
      }
    }
  };

  const begin = (action: Action, button: HTMLButtonElement) => {
    opener.current = button;
    if (memories.length === 1) void run(action, memories);
    else setChooser(action);
  };

  const closeChooser = () => {
    setChooser(null);
    opener.current?.focus();
  };

  const save = async () => {
    if (!correcting || draft.trim() === "") return;
    setBusy(true);
    try {
      await engine.correctMemory(correcting.id, draft.trim());
      await refresh();
      setCorrecting(null);
      onCorrected(draft.trim());
      onResult(t("ask.wrong.done"));
      resultRef.current?.focus();
    } catch {
      fail();
    } finally {
      setBusy(false);
    }
  };

  const cancel = () => {
    setCorrecting(null);
    opener.current?.focus();
  };

  const disabled = memories.length === 0 || busy;
  return (
    <div className={styles.actionsBlock}>
      {showButtons ? (
        <fieldset className={styles.actions}>
          <legend className="sr-only">{t("ask.actions")}</legend>
          <Button
            variant="secondary"
            disabled={disabled}
            onClick={(e) => begin("wrong", e.currentTarget)}
          >
            {t("ask.wrong")}
          </Button>
          <Button
            variant="secondary"
            disabled={disabled || allKeptForever}
            onClick={(e) => begin("keepForever", e.currentTarget)}
          >
            {allKeptForever ? t("memory.flag.keepForever") : t("ask.keepForever")}
          </Button>
          <button
            type="button"
            className={styles.quiet}
            disabled={disabled}
            onClick={(e) => begin("forget", e.currentTarget)}
          >
            {t("ask.forget")}
          </button>
        </fieldset>
      ) : null}

      {chooser ? (
        <fieldset
          className={styles.chooser}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              closeChooser();
            }
          }}
        >
          <legend className={styles.chooserTitle}>{t("ask.chooser.title")}</legend>
          {memories.map((memory, index) => (
            <button
              key={memory.id}
              ref={index === 0 ? firstChoice : undefined}
              type="button"
              className={styles.choice}
              onClick={() => void run(chooser, [memory])}
            >
              {memory.sentence}
            </button>
          ))}
          {chooser !== "wrong" ? (
            <button
              type="button"
              className={styles.choice}
              onClick={() => void run(chooser, memories)}
            >
              {t("ask.chooser.all")}
            </button>
          ) : null}
        </fieldset>
      ) : null}

      {correcting ? (
        <form
          className={styles.correction}
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              cancel();
            }
          }}
        >
          <label className={styles.correctionLabel} htmlFor="ask-correction">
            {t("ask.wrong.label")}
          </label>
          <textarea
            id="ask-correction"
            ref={field}
            className={styles.correctionField}
            rows={2}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <div className={styles.correctionActions}>
            <Button type="submit" variant="primary" disabled={busy || draft.trim() === ""}>
              {t("ask.wrong.save")}
            </Button>
            <Button variant="quiet" onClick={cancel}>
              {t("ask.wrong.cancel")}
            </Button>
          </div>
        </form>
      ) : null}

      <p ref={resultRef} className={styles.result} role="status" aria-live="polite" tabIndex={-1}>
        {result}
      </p>

      {forgetting ? (
        <ForgetDialog
          memoryIds={forgetting}
          onClose={() => {
            setForgetting(null);
            // The dialog hands focus back to a button that is gone: the result is what is left.
            if (forgotten.current) setTimeout(() => resultRef.current?.focus(), 0);
          }}
          onDone={() => {
            // Called once, as soon as the engine has forgotten; the dialog says so until closed.
            forgotten.current = true;
            const ids = forgetting;
            onForgotten(ids);
            for (const key of ["askMemories", "needsYou", "status", "page", "pages", "items"]) {
              void client.invalidateQueries({ queryKey: [key] });
            }
          }}
        />
      ) : null}
    </div>
  );
}
