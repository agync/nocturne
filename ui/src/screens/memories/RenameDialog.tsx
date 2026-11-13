import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { TextInput } from "../../components/TextInput/TextInput";
import { t } from "../../copy/t";
import type { EntityView, MergeOutcome, RenameOutcome } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Dialogs.module.css";

interface RenameDialogProps {
  entity: EntityView;
  onClose: () => void;
  /** A change of name was made; `changeId` is null when the name was the same. */
  onRenamed: (entity: EntityView, changeId: string | null) => void;
  onMerged: (outcome: MergeOutcome) => void;
}

/**
 * Renaming a person, place or thing. A name that something else already has is not refused:
 * the same dialog asks whether they are one and the same, and merges them if so.
 */
export function RenameDialog({ entity, onClose, onRenamed, onMerged }: RenameDialogProps) {
  const engine = useEngine();
  const [name, setName] = useState(entity.name);
  const [clash, setClash] = useState<EntityView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const field = useRef<HTMLInputElement>(null);

  const rename = async () => {
    const wanted = name.trim();
    if (wanted === "") {
      setError(t("rename.error.empty"));
      field.current?.focus();
      return;
    }
    if (wanted === entity.name) {
      onClose();
      return;
    }
    setBusy(true);
    setError(undefined);
    try {
      const outcome: RenameOutcome = await engine.renameEntity(entity.id, wanted);
      if (outcome.type === "exists") {
        setClash(outcome.entity);
      } else {
        onRenamed(outcome.entity, outcome.changeId);
      }
    } catch {
      setError(t("memories.failed"));
    } finally {
      setBusy(false);
    }
  };

  const merge = async (into: EntityView) => {
    setBusy(true);
    setError(undefined);
    try {
      onMerged(await engine.mergeEntities(entity.id, into.id));
    } catch {
      setError(t("memories.failed"));
    } finally {
      setBusy(false);
    }
  };

  if (clash) {
    return (
      <Dialog title={t("rename.merge.title", { name: clash.name })} onClose={onClose} locked={busy}>
        <div className={styles.body}>
          <p>{t("rename.merge.body", { name: clash.name, old: entity.name })}</p>
          <p className={styles.note}>{t("rename.merge.moves", { count: entity.memoryCount })}</p>
          {error ? (
            <p className={styles.problem} role="alert">
              {error}
            </p>
          ) : null}
          <div className={styles.actions}>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                setClash(null);
                setTimeout(() => field.current?.focus(), 0);
              }}
            >
              {t("rename.merge.keep")}
            </Button>
            <Button variant="primary" disabled={busy} onClick={() => void merge(clash)}>
              {t("rename.merge.confirm")}
            </Button>
          </div>
        </div>
      </Dialog>
    );
  }

  return (
    <Dialog title={t("rename.title")} onClose={onClose} locked={busy} initialFocus={field}>
      <form
        className={styles.body}
        onSubmit={(event) => {
          event.preventDefault();
          void rename();
        }}
      >
        <TextInput
          label={t("rename.label")}
          hint={t("rename.hint")}
          value={name}
          inputRef={field}
          maxLength={120}
          error={error}
          onChange={(value) => {
            setName(value);
            setError(undefined);
          }}
        />
        <div className={styles.actions}>
          <Button variant="secondary" onClick={onClose} disabled={busy}>
            {t("dialog.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={busy}>
            {t("rename.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
