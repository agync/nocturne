import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { TextInput } from "../../components/TextInput/TextInput";
import { t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import styles from "./Privacy.module.css";

/** The words the engine wants back before it removes everything. */
export const DELETE_PHRASE = "delete everything";

interface Props {
  title: string;
  body: string;
  confirmLabel: string;
  /** The host of the provider Online answers was set up with, when it was. */
  online?: string | null;
  onClose: () => void;
  /** The engine has removed everything. */
  onDeleted: () => void;
}

/** Deleting everything, said in words and confirmed by typing them. Enter does nothing by itself. */
export function DeleteEverythingDialog({
  title,
  body,
  confirmLabel,
  online,
  onClose,
  onDeleted,
}: Props) {
  const engine = useEngine();
  const field = useRef<HTMLInputElement>(null);
  const [typed, setTyped] = useState("");
  const [working, setWorking] = useState(false);
  const [failed, setFailed] = useState(false);
  const matches = typed.trim() === DELETE_PHRASE;

  const go = async () => {
    if (!matches || working) return;
    setWorking(true);
    setFailed(false);
    try {
      await engine.deleteEverything(DELETE_PHRASE);
      onDeleted();
    } catch {
      setFailed(true);
      setWorking(false);
    }
  };

  return (
    <Dialog title={title} initialFocus={field} locked={working} onClose={onClose}>
      <p className={styles.dialogBody}>{body}</p>
      {online ? (
        <p className={styles.dialogBody}>{t("privacy.delete.dialog.online", { host: online })}</p>
      ) : null}
      <TextInput
        label={t("privacy.delete.field", { phrase: DELETE_PHRASE })}
        value={typed}
        inputRef={field}
        autoComplete="off"
        spellCheck={false}
        onChange={setTyped}
        error={failed ? t("privacy.delete.failed") : undefined}
      />
      <div className={styles.dialogActions}>
        <Button variant="secondary" disabled={working} onClick={onClose}>
          {t("privacy.delete.keep")}
        </Button>
        <Button
          variant="forget"
          confirming
          disabled={!matches || working}
          aria-busy={working}
          onClick={() => void go()}
        >
          {working ? t("privacy.delete.working") : confirmLabel}
        </Button>
      </div>
    </Dialog>
  );
}
