import { useId, useState } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { t } from "../../copy/t";
import type { ChangePreview, Fit, Job, ModelChoice, ModelsView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { useOnline } from "../online/useOnline";
import { Confirm } from "./Confirm";
import { gb, JOB_TITLE, PICK_INTRO, PICK_SMALLER, PICK_TITLE } from "./jobs";
import styles from "./Models.module.css";
import { useChoices, useModels, useModelsCommands } from "./useModels";
import { reasonLine, Why } from "./Why";

const ONLINE = "online";

const FIT_WORD = {
  fits: "models.fit.fits",
  tight: "models.fit.tight",
  wontFit: "models.fit.wontFit",
  cantUse: "models.fit.cantUse",
} as const satisfies Record<Fit, string>;

/** A mark that never relies on colour: an icon and a word. */
function FitMark({ id, fit }: { id: string; fit: Fit }) {
  return (
    <span id={id} className={styles.fit} data-fit={fit}>
      <svg
        width="15"
        height="15"
        viewBox="0 0 24 24"
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {fit === "fits" ? <path d="M20 6 9 17l-5-5" /> : null}
        {fit === "tight" ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor" />
          </>
        ) : null}
        {fit === "wontFit" || fit === "cantUse" ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="m5.7 5.7 12.6 12.6" />
          </>
        ) : null}
      </svg>
      {t(FIT_WORD[fit])}
    </span>
  );
}

function meta(choice: ModelChoice, job: Job): string {
  const sizes = { size: gb(choice.sizeBytes), memory: gb(choice.memoryBytes) };
  const base =
    job === "aloud"
      ? t("models.pick.metaAlone", sizes)
      : t(choice.onDisk ? "models.pick.metaHere" : "models.pick.metaGet", sizes);
  return choice.smaller ? `${base} · ${t(PICK_SMALLER[job])}` : base;
}

function Option({
  choice,
  view,
  checked,
  onPick,
  locked,
}: {
  choice: ModelChoice;
  view: ModelsView;
  checked: boolean;
  onPick: () => void;
  locked: boolean;
}) {
  const [open, setOpen] = useState(false);
  const whyId = useId();
  const grey = choice.fit === "wontFit" || choice.fit === "cantUse";
  const line = reasonLine(choice);
  const longer = grey && choice.reason !== null;
  const described = [`${whyId}-meta`, `${whyId}-fit`, line ? `${whyId}-line` : null]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={styles.option} data-on={checked} data-grey={grey}>
      <input
        type="radio"
        name="pick"
        id={`${whyId}-radio`}
        checked={checked}
        disabled={(grey && !choice.current) || locked}
        onChange={onPick}
        aria-describedby={described}
      />
      <span className={styles.optionText}>
        <label htmlFor={`${whyId}-radio`} className={styles.nameRow}>
          <span className={styles.mono}>{choice.name}</span>
          {choice.current ? <span className={styles.tag}>{t("models.tag.current")}</span> : null}
          {choice.recommended ? (
            <span className={styles.tag}>{t("models.tag.recommended")}</span>
          ) : null}
          {choice.tested ? null : <span className={styles.tag}>{t("models.tag.untested")}</span>}
        </label>
        <span id={`${whyId}-meta`} className={styles.hint}>
          {meta(choice, choice.job)}
        </span>
        {line ? (
          <span className={styles.hint}>
            <span id={`${whyId}-line`} className={styles.strong}>
              {line}
            </span>
            {longer ? (
              <>
                {" "}
                <button
                  type="button"
                  className={styles.linkButton}
                  aria-expanded={open}
                  aria-controls={open ? whyId : undefined}
                  onClick={() => setOpen(!open)}
                >
                  {t("models.why.link")}
                </button>
              </>
            ) : null}
          </span>
        ) : null}
      </span>
      <FitMark id={`${whyId}-fit`} fit={choice.fit} />
      {open ? <Why id={whyId} choice={choice} budgetBytes={view.budgetBytes} /> : null}
    </div>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <>
      <span className={`${styles.label} ${styles.groupLabel}`}>{title}</span>
      {children}
    </>
  );
}

/** The Online option of Answering, in the list but never chosen here: it is set up in its own place. */
function OnlineOption({
  checked,
  onPick,
  locked,
  answeringName,
}: {
  checked: boolean;
  onPick: () => void;
  locked: boolean;
  answeringName: string;
}) {
  const online = useOnline().data;
  const on = online?.state.type === "on";
  const id = useId();
  return (
    <div className={styles.option} data-on={checked} data-grey="false">
      <input
        type="radio"
        name="pick"
        id={`${id}-radio`}
        checked={checked}
        disabled={locked}
        onChange={onPick}
        aria-describedby={`${id}-meta`}
      />
      <span className={styles.optionText}>
        <label htmlFor={`${id}-radio`} className={styles.nameRow}>
          <span className={styles.strongLabel}>{t("models.online.name")}</span>
          <span className={styles.tag}>
            {t(on ? "models.online.tagOn" : "models.online.tagOff")}
          </span>
        </label>
        <span id={`${id}-meta`} className={styles.hint}>
          {t("models.online.meta", { name: answeringName })}
        </span>
        {checked && !on ? (
          <div className={styles.why}>
            <span>{t("models.online.m9")}</span>
            <a className={`${styles.linkButton} hit`} href="#/settings/online/setup">
              {t("models.online.setup")}
            </a>
          </div>
        ) : null}
      </span>
      <a className={`${styles.link} hit`} href="#/settings/online">
        {t("models.online.settings")}
      </a>
    </div>
  );
}

/** #/settings/models/<job> (M3, M4, M5, M9): every choice for one job, with what fits. */
export function Picker({ job }: { job: Job }) {
  const engine = useEngine();
  const models = useModels({ screen: true });
  const view = models.data;
  const found = useChoices(job);
  const choices = found.data;
  const error = found.error || models.error;
  const retry = () => {
    models.refetch();
    found.refetch();
  };
  const { change } = useModelsCommands();
  const online = useOnline().data;
  const [picked, setPicked] = useState<string | null>(null);
  const [preview, setPreview] = useState<ChangePreview | null>(null);
  const [problem, setProblem] = useState(false);
  const [previewing, setPreviewing] = useState(false);

  const current = choices?.find((c) => c.current);
  const onlineOn = online?.state.type === "on";
  const selected = picked ?? (job === "answering" && onlineOn ? ONLINE : (current?.id ?? null));
  const informational = job === "aloud";
  const chosen = choices?.find((c) => c.id === selected);
  const running =
    view !== undefined &&
    ((view.change !== null && view.change.phase.type !== "ready") ||
      view.reindex?.phase === "running");
  const canUse =
    chosen !== undefined && !chosen.current && !informational && !running && !previewing;

  const here =
    choices?.filter((c) => c.current || (c.onDisk && c.fit !== "wontFit" && c.fit !== "cantUse")) ??
    [];
  const get =
    choices?.filter((c) => !c.current && !c.onDisk && c.fit !== "wontFit" && c.fit !== "cantUse") ??
    [];
  const cant =
    choices?.filter((c) => !c.current && (c.fit === "wontFit" || c.fit === "cantUse")) ?? [];

  const ask = async () => {
    if (!chosen) return;
    setProblem(false);
    setPreviewing(true);
    try {
      setPreview(await engine.previewModelChange(job, chosen.id));
    } catch {
      setProblem(true);
    } finally {
      setPreviewing(false);
    }
  };

  const crumb = t("advanced.models.crumb", { job: t(JOB_TITLE[job]) });

  return (
    <section className={styles.page}>
      <div className={styles.back}>
        <BackLink to="/settings/models" label={t("advanced.models.title")} />
      </div>
      <span className={styles.label}>{crumb}</span>
      <h1 className="t-page-title">{t(PICK_TITLE[job])}</h1>
      <p className={styles.body}>{t(PICK_INTRO[job])}</p>
      <p className={styles.legendLine}>
        <span>
          <b className={styles.strong}>{t("models.fit.fits")}</b> {t("models.fit.fitsHint")}
        </span>
        <span>
          <b className={styles.strong}>{t("models.fit.tight")}</b> {t("models.fit.tightHint")}
        </span>
        <span>
          <b className={styles.strong}>{t("models.fit.wontFit")}</b> {t("models.fit.wontFitHint")}
        </span>
      </p>

      {error ? (
        <div className={styles.card} role="alert">
          <p className={styles.body}>{t("models.pick.error")}</p>
          <div className={styles.actions}>
            <Button onClick={retry}>{t("models.read.retry")}</Button>
          </div>
        </div>
      ) : !choices || !view ? (
        <div className={styles.skeleton} role="status">
          <span className={styles.nameRow}>{t("models.pick.loading")}</span>
          <span className={styles.skel} style={{ width: "70%" }} />
          <span className={styles.skel} style={{ width: "55%" }} />
          <span className={styles.skel} style={{ width: "62%" }} />
        </div>
      ) : (
        <>
          <fieldset className={styles.picker}>
            <legend className="sr-only">{t(PICK_TITLE[job])}</legend>
            {[
              { title: t("models.group.here"), rows: here },
              { title: t("models.group.get"), rows: get },
              { title: t("models.group.cant"), rows: cant },
            ]
              .filter((g) => g.rows.length > 0)
              .map((g) => (
                <Group key={g.title} title={g.title}>
                  {g.rows.map((c) => (
                    <Option
                      key={c.id}
                      choice={c}
                      view={view}
                      checked={selected === c.id}
                      locked={informational || previewing}
                      onPick={() => setPicked(c.id)}
                    />
                  ))}
                </Group>
              ))}
            {job === "answering" ? (
              <Group title={t("models.group.online")}>
                <OnlineOption
                  checked={selected === ONLINE}
                  locked={previewing}
                  onPick={() => setPicked(ONLINE)}
                  answeringName={current?.name ?? ""}
                />
              </Group>
            ) : null}
          </fieldset>
          {informational ? (
            <p className={styles.body}>{tc("models.aloud.only", { name: current?.name ?? "" })}</p>
          ) : null}
          {problem ? (
            <p className={styles.body} role="alert">
              {t("models.confirm.failed")}
            </p>
          ) : null}
          {running ? (
            <p className={styles.hint}>
              {t("models.pick.running")}{" "}
              <a className={`${styles.linkButton} hit`} href="#/settings/models">
                {t("models.pick.runningLink")}
              </a>
            </p>
          ) : null}
          <div className={styles.actions}>
            <Button variant="primary" disabled={!canUse} onClick={() => void ask()}>
              {t("models.pick.use")}
            </Button>
            <a className={`${styles.link} hit`} href="#/settings/models">
              {t("models.pick.back")}
            </a>
            <span className={styles.hint}>{t("models.pick.note")}</span>
          </div>
        </>
      )}

      {preview ? (
        <Confirm
          preview={preview}
          name={choices?.find((c) => c.id === preview.model)?.name ?? preview.model}
          fromName={current?.name ?? preview.from}
          onClose={() => setPreview(null)}
          onConfirm={async (allowDownload) => {
            await change(job, preview.model, allowDownload);
            setPreview(null);
            window.location.hash = "#/settings/models";
          }}
        />
      ) : null}
    </section>
  );
}
