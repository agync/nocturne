import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { t } from "../../copy/t";
import type { ForgetPreview, ForgetReport } from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine } from "../../engine/context";
import styles from "./Checkin.module.css";

interface DeleteCheckinProps {
  itemId: string;
  /** "Low · tired". */
  feel: string;
  /** "8:05 am". */
  time: string;
  hasLine: boolean;
  onClose: () => void;
  onDone: (report: ForgetReport) => void;
}

type Phase = "loading" | "ready" | "working" | "failedPreview";

/** "Delete this check-in?": the short form of the delete dialog. Focus starts on Keep it. */
export function DeleteCheckin({
  itemId,
  feel,
  time,
  hasLine,
  onClose,
  onDone,
}: DeleteCheckinProps) {
  const engine = useEngine();
  const keep = useRef<HTMLElement | null>(null);
  const alive = useRef(true);
  const [phase, setPhase] = useState<Phase>("loading");
  const [preview, setPreview] = useState<ForgetPreview | null>(null);
  const [refreshed, setRefreshed] = useState(false);
  const [failed, setFailed] = useState(false);

  const prepare = async () => {
    setPhase("loading");
    try {
      const made = await engine.previewForget([], true, { also: [itemId], pick: true });
      if (!alive.current) return;
      setPreview(made);
      setPhase("ready");
    } catch {
      if (alive.current) setPhase("failedPreview");
    }
  };

  useEffect(() => {
    alive.current = true;
    void prepare();
    return () => {
      alive.current = false;
    };
    // The list is made once; a refresh after an expired token asks again by hand.
    // biome-ignore lint/correctness/useExhaustiveDependencies: see above
  }, []);

  const forget = async () => {
    if (!preview || phase !== "ready") return;
    setPhase("working");
    setFailed(false);
    try {
      const report = await engine.forget(preview.token);
      onDone(report);
    } catch (error) {
      if (!alive.current) return;
      const e = toEngineError(error);
      if (/expire/i.test(`${e.code} ${e.detail}`)) setRefreshed(true);
      else setFailed(true);
      await prepare();
    }
  };

  const lines = preview?.reflectionLines ?? 0;
  const idle = phase === "ready";

  return (
    <Dialog
      title={t("journal.deleteCheckin.title")}
      initialFocus={keep}
      locked={phase === "working"}
      onClose={onClose}
    >
      <p>
        {t(hasLine ? "journal.deleteCheckin.body" : "journal.deleteCheckin.bodyBare", {
          feel,
          time,
        })}
      </p>
      {phase === "failedPreview" ? (
        <p className={styles.problem} role="alert">
          {t("forget.error.preview")}
        </p>
      ) : null}
      {refreshed && idle ? (
        <p className={styles.stays} role="status">
          {t("forget.refreshed")}
        </p>
      ) : null}
      {failed && idle ? (
        <p className={styles.problem} role="alert">
          {t("forget.error.forget")}
        </p>
      ) : null}
      <ul className={styles.stays}>
        {lines > 0 ? <li>{t("journal.forget.cites", { count: lines })}</li> : null}
        <li>{t("forget.stays.out")}</li>
      </ul>
      <div
        className={styles.actions}
        ref={(el) => {
          keep.current = el?.querySelector("button") ?? null;
        }}
      >
        <Button variant="secondary" disabled={phase === "working"} onClick={onClose}>
          {t("journal.deleteCheckin.keep")}
        </Button>
        {phase === "failedPreview" ? (
          <Button variant="secondary" onClick={() => void prepare()}>
            {t("button.tryAgain")}
          </Button>
        ) : (
          <Button
            variant="forget"
            confirming
            disabled={!idle}
            aria-busy={phase === "working"}
            onClick={() => void forget()}
          >
            {phase === "working" ? t("forget.working") : t("journal.deleteCheckin.button")}
          </Button>
        )}
      </div>
    </Dialog>
  );
}
