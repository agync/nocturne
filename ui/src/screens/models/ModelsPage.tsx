import { useRef, useState } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import type { Job, JobView, ModelsView } from "../../engine/bindings";
import { useOnline } from "../online/useOnline";
import { Banners } from "./Banners";
import { changedJobs, gb, JOB_HINT, JOB_ORDER, JOB_TITLE, TIER_NAME } from "./jobs";
import { MemoryMeter } from "./MemoryMeter";
import styles from "./Models.module.css";
import { useModels, useModelsCommands } from "./useModels";

/** "Answering questions and Reading aloud": the jobs a person has changed, in words. */
function listJobs(jobs: Job[]): string {
  const names = jobs.map((j) => t(JOB_TITLE[j]));
  const last = names.pop();
  return names.length === 0 ? (last ?? "") : `${names.join(", ")} and ${last}`;
}

function OnlineLine() {
  const state = useOnline().data?.state.type;
  const word =
    state === "on"
      ? t("models.online.state.on")
      : state === "off" || state === undefined
        ? t("models.online.state.off")
        : state === "problem"
          ? t("models.online.state.problem")
          : t("models.online.state.other");
  return (
    <span className={styles.hint}>
      {t("models.online.line")}{" "}
      <a className={`${styles.linkButton} hit`} href="#/settings/online">
        {t("models.online.link", { state: word })}
      </a>
    </span>
  );
}

function Row({
  view,
  job,
  mine,
  fail,
}: {
  view: ModelsView;
  job: JobView;
  mine: ReturnType<typeof useModelsCommands>["act"];
  fail: () => void;
}) {
  const alone = job.job === "aloud";
  return (
    <li className={styles.job}>
      <div className={styles.jobHead}>
        <span className={styles.jobName}>{t(JOB_TITLE[job.job])}</span>
        <span className={styles.hint}>{t(JOB_HINT[job.job])}</span>
      </div>
      <div className={styles.jobBody}>
        <span className={styles.nameRow}>
          <span className={styles.mono}>{job.name}</span>
          {job.recommended ? (
            <span className={styles.tag}>{t("models.tag.recommended")}</span>
          ) : null}
          {job.tested ? null : <span className={styles.tag}>{t("models.tag.untested")}</span>}
        </span>
        <span className={styles.hint}>
          {t(alone ? "models.job.metaAlone" : "models.job.meta", {
            size: gb(job.sizeBytes),
            memory: gb(job.memoryBytes),
          })}
        </span>
        {job.job === "answering" ? <OnlineLine /> : null}
        <Banners view={view} job={job.job} act={mine} fail={fail} />
      </div>
      <a
        className={`${styles.link} hit`}
        href={`#/settings/models/${job.job}`}
        aria-label={t("models.job.changeAria", { job: t(JOB_TITLE[job.job]) })}
      >
        {t("models.job.change")}
      </a>
    </li>
  );
}

/** #/settings/models (M1, M2): what runs each job, memory at its busiest, and any change under way. */
export function ModelsPage() {
  const { data: view, failure, refetch } = useModels({ screen: true });
  const { act } = useModelsCommands();
  const heading = useRef<HTMLHeadingElement>(null);
  const [failed, setFailed] = useState(false);
  const fail = () => setFailed(true);
  const run: typeof act = async (command) => {
    setFailed(false);
    const next = await act(command);
    // The card that held the button is gone, so focus goes to the heading, not nowhere.
    if (command === "cancel" || command === "keepOld") heading.current?.focus();
    return next;
  };

  return (
    <section className={styles.page}>
      <div className={styles.back}>
        <BackLink to="/settings" label={t("models.back")} />
      </div>
      <div className={styles.titleRow}>
        <h1 className="t-page-title" ref={heading} tabIndex={-1}>
          {t("advanced.models.title")}
        </h1>
        <span className={styles.tag}>{t("models.tag")}</span>
      </div>
      {view ? (
        <>
          <p className={styles.lede}>{t("models.lede", { ram: gb(view.ramBytes) })}</p>
          {failed ? (
            <p className={styles.body} role="alert">
              {t("models.confirm.failed")}
            </p>
          ) : null}
          <Start view={view} act={run} fail={fail} />
          <MemoryMeter view={view} />
          <ul className={styles.jobs}>
            {JOB_ORDER.map((job) => {
              const row = view.jobs.find((j) => j.job === job);
              return row ? <Row key={job} view={view} job={row} mine={run} fail={fail} /> : null;
            })}
          </ul>
        </>
      ) : failure ? (
        <div className={styles.card} role="alert">
          <p className={styles.body}>
            {t(failure.code === "unavailable" ? "advanced.models.unavailable" : "advanced.error")}
          </p>
          <p className={styles.body}>{t("advanced.errorDetail", { detail: failure.detail })}</p>
          <div className={styles.actions}>
            <Button onClick={refetch}>{t("models.read.retry")}</Button>
          </div>
        </div>
      ) : (
        <p className={styles.body} role="status">
          {t("models.loading")}
        </p>
      )}
    </section>
  );
}

function Start({
  view,
  act,
  fail,
}: {
  view: ModelsView;
  act: ReturnType<typeof useModelsCommands>["act"];
  fail: () => void;
}) {
  const tier = t(TIER_NAME[view.preset]);
  const changed = changedJobs(view);
  return (
    <div className={styles.panel}>
      <span className={styles.panelText}>
        <span className={styles.strongLabel}>{t("models.start.label")}</span>
        <span className={styles.hint}>
          {view.custom
            ? t("models.start.customHint", { jobs: listJobs(changed), tier })
            : t("models.start.tierHint", { tier })}
        </span>
      </span>
      <span className={styles.actions}>
        {view.custom ? (
          <>
            <span className={styles.tag}>{t("models.start.custom")}</span>
            <Button onClick={() => void act("reset").catch(fail)}>
              {t("models.start.reset", { tier })}
            </Button>
          </>
        ) : (
          <span className={styles.strongLabel}>{t("models.start.tier", { tier })}</span>
        )}
      </span>
    </div>
  );
}
