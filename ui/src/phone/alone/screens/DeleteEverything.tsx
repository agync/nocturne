import { useId, useState } from "react";
import { t } from "../../../copy/t";
import { useHeading } from "../../native/screens/parts";
import { Icon, Page } from "../../screens/ui";

/** The words to type, as the computer's own check has them. */
export const DELETE_PHRASE = "delete everything";

/** Delete everything on this phone: the computer's typed check, worded for this phone. */
export function DeleteEverything({
  onDelete,
  onKeep,
}: {
  onDelete(phrase: string): Promise<void>;
  onKeep(): void;
}) {
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const heading = useHeading("delete");
  const id = useId();
  const ready = typed.trim().toLowerCase() === DELETE_PHRASE;

  const go = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await onDelete(DELETE_PHRASE);
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <Page
      bar={
        <header className="ph-bar ph-bar--back">
          <button
            type="button"
            className="ph-ib ph-ib--ink"
            onClick={onKeep}
            aria-label={t("phone.cancel")}
          >
            <Icon name="close" />
          </button>
          <span className="ph-bar-title ph-serif" aria-hidden="true">
            {t("phone.alone.delete.bar")}
          </span>
        </header>
      }
    >
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.delete.title")}
        </h1>
        <p className="ph-body ph-ink2">{t("phone.alone.delete.body")}</p>
        <div className="ph-field">
          <label htmlFor={id}>{t("phone.alone.delete.typed")}</label>
          <input
            id={id}
            className="ph-input"
            type="text"
            autoComplete="off"
            autoCapitalize="none"
            spellCheck={false}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
          />
        </div>
        {failed ? (
          <p className="ph-error" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <button
          type="button"
          className="ph-btn ph-btn--ember"
          aria-disabled={!ready || busy ? true : undefined}
          onClick={() => void go()}
        >
          {t("phone.alone.delete.yes")}
        </button>
        <button type="button" className="ph-textbtn" onClick={onKeep}>
          {t("phone.alone.delete.no")}
        </button>
      </footer>
    </Page>
  );
}
