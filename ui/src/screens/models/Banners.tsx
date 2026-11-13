import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { Progress } from "../../components/Progress/Progress";
import { t } from "../../copy/t";
import type { Job, ModelsView } from "../../engine/bindings";
import { gb, JOB_DOING, jobOf } from "./jobs";
import styles from "./Models.module.css";
import type { useModelsCommands } from "./useModels";

type Act = ReturnType<typeof useModelsCommands>["act"];

const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

const minutes = (seconds: number) => Math.max(1, Math.round(seconds / 60));

/** The download of a new model: how far, and a way to pause or stop. The current one keeps working. */
function Download({ view, act, fail }: { view: ModelsView; act: Act; fail: () => void }) {
  const [asking, setAsking] = useState(false);
  const change = view.change;
  if (!change || change.phase.type === "ready") return null;
  const next = change.model;
  const fromName = nameOf(view, change.job, change.from);
  const run = (command: "pause" | "resume" | "cancel") => act(command).catch(fail);

  if (change.phase.type === "failed") {
    const { received, total } = change.phase;
    return (
      <div className={styles.card} role="alert" data-bad="true">
        <p className={styles.cardTitle} data-bad="true">
          {t("models.dl.failedTitle", { received: gb(received), total: gb(total) })}
        </p>
        <p className={`${styles.body} ${styles.strong}`}>
          {t("models.dl.failedBody", { from: fromName, received: gb(received) })}
        </p>
        <div className={styles.actions}>
          <Button onClick={() => void run("resume")}>{t("models.dl.retry")}</Button>
          <Button variant="quiet" onClick={() => void run("cancel")}>
            {t("models.dl.cancelRemove")}
          </Button>
        </div>
      </div>
    );
  }

  const { received, total, paused } = change.phase;
  const title = paused
    ? t("models.dl.titlePaused", { name: next })
    : change.resumed
      ? t("models.dl.titleResumed", { name: next })
      : t("models.dl.title", { name: next });
  const hintId = paused
    ? "models.dl.hintPaused"
    : change.resumed
      ? "models.dl.hintResumed"
      : "models.dl.hint";
  return (
    <div className={styles.card}>
      <span className="sr-only" role="status">
        {title}
      </span>
      <Progress
        value={received}
        max={total}
        label={title}
        detail={t("models.dl.amount", { received: gb(received), total: gb(total) })}
        paused={paused}
      />
      <p className={styles.hint}>{t(hintId, { from: fromName })}</p>
      <div className={styles.actions}>
        <Button onClick={() => void run(paused ? "resume" : "pause")}>
          {paused ? t("models.dl.resume") : t("models.dl.pause")}
        </Button>
        <Button variant="quiet" onClick={() => setAsking(true)}>
          {t("models.dl.cancel")}
        </Button>
      </div>
      {asking ? (
        <Dialog title={t("models.dl.cancelTitle", { name: next })} onClose={() => setAsking(false)}>
          <p className={styles.dialogBody}>
            {t("models.dl.cancelBody", { received: gb(received), from: fromName })}
          </p>
          <div className={styles.dialogActions}>
            <Button onClick={() => setAsking(false)}>{t("models.dl.keep")}</Button>
            <Button
              variant="primary"
              onClick={() => {
                setAsking(false);
                void run("cancel");
              }}
            >
              {t("models.dl.stop")}
            </Button>
          </div>
        </Dialog>
      ) : null}
    </div>
  );
}

/** The name a model is listed by: the current one is in the jobs; a model from before is its id. */
function nameOf(view: ModelsView, job: Job, id: string): string {
  const current = jobOf(view, job);
  return current && current.model === id ? current.name : id;
}

/** The memories being read again for the new finding model; the old way keeps working meanwhile. */
function Reread({ view, act, fail }: { view: ModelsView; act: Act; fail: () => void }) {
  const reindex = view.reindex;
  if (!reindex) return null;
  if (reindex.phase === "running") {
    const eta = reindex.etaSeconds;
    return (
      <div className={styles.card}>
        <span className="sr-only" role="status">
          {t("models.ri.title", { name: reindex.model })}
        </span>
        <Progress
          value={reindex.done}
          max={reindex.total}
          label={t("models.ri.title", { name: reindex.model })}
          detail={t("models.ri.amount", { done: count(reindex.done), total: count(reindex.total) })}
        />
        <p className={styles.hint}>
          {eta === null
            ? t("models.ri.hintNoEta")
            : t("models.ri.hint", { eta: t("models.ri.eta", { minutes: minutes(eta) }) })}
        </p>
        <div className={styles.actions}>
          <Button variant="quiet" onClick={() => void act("cancelReindex").catch(fail)}>
            {t("models.ri.cancel")}
          </Button>
        </div>
      </div>
    );
  }
  if (reindex.phase === "cancelled") {
    return (
      <div className={styles.card} role="status">
        <p className={styles.body}>{t("models.ri.cancelled", { from: reindex.from })}</p>
      </div>
    );
  }
  return null;
}

/** After a switch: keep the old one (the default, and focused) or remove it to free its space. */
function Old({ view, act, fail }: { view: ModelsView; act: Act; fail: () => void }) {
  const old = view.old;
  if (!old) return null;
  const now = jobOf(view, old.job);
  const size = gb(old.bytes);
  if (old.removed) {
    return (
      <div className={styles.card} role="status">
        <p className={styles.body}>{t("models.old.removed", { from: old.model, size })}</p>
      </div>
    );
  }
  const reread = old.job === "finding" && view.reindex?.phase === "done";
  return (
    <div className={styles.card} role="status">
      {reread ? (
        <p className={styles.cardTitle}>
          {t("models.ri.done", { name: now?.name ?? "", total: count(view.reindex?.total ?? 0) })}
        </p>
      ) : (
        <>
          <p className={styles.cardTitle}>
            {t("models.old.title", { name: now?.name ?? "", doing: t(JOB_DOING[old.job]) })}
          </p>
          <p className={styles.body}>{t("models.old.body", { from: old.model, size })}</p>
        </>
      )}
      <div className={styles.actions}>
        <Button onClick={() => void act("keepOld").catch(fail)}>{t("models.old.keep")}</Button>
        <Button onClick={() => void act("removeOld").catch(fail)}>
          {reread
            ? t("models.old.removeNamed", { from: old.model, size })
            : t("models.old.remove", { size })}
        </Button>
      </div>
    </div>
  );
}

/** Everything under way or just finished for one job, in the order a person needs it. */
export function Banners({
  view,
  job,
  act,
  fail,
}: {
  view: ModelsView;
  job: Job;
  act: Act;
  fail: () => void;
}) {
  return (
    <>
      {view.change?.job === job ? <Download view={view} act={act} fail={fail} /> : null}
      {job === "finding" ? <Reread view={view} act={act} fail={fail} /> : null}
      {view.old?.job === job ? <Old view={view} act={act} fail={fail} /> : null}
    </>
  );
}
