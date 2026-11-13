import { useId, useState } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
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
function FitMark({ fit }: { fit: Fit }) {
  return (
    <span className={styles.fit} data-fit={fit}>
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
  informational,
}: {
  choice: ModelChoice;
  view: ModelsView;
  checked: boolean;
  onPick: () => void;
  informational: boolean;
}) {
  const [open, setOpen] = useState(false);
  const whyId = useId();
  const grey = choice.fit === "wontFit" || choice.fit === "cantUse";
  const line = reasonLine(choice);
  const longer = grey && choice.reason !== null;
  return (
    <div className={styles.option} data-on={checked} data-grey={grey}>
      <input
        type="radio"
        name="pick"
        id={`${whyId}-radio`}
        checked={checked}
        disabled={grey}
        readOnly={informational}
        onChange={onPick}
        aria-describedby={`${whyId}-meta`}
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
            <span className={styles.strong}>{line}</span>
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
      <FitMark fit={choice.fit} />
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
  answeringName,
}: {
  checked: boolean;
  onPick: () => void;
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
  const { data: view } = useModels({ screen: true });
  const { data: choices, error } = useChoices(job);
  const { change } = useModelsCommands();
  const online = useOnline().data;
  const [picked, setPicked] = useState<string | null>(null);
  const [preview, setPreview] = useState<ChangePreview | null>(null);
  const [problem, setProblem] = useState(false);

  const current = choices?.find((c) => c.current);
  const onlineOn = online?.state.type === "on";
  const selected = picked ?? (job === "answering" && onlineOn ? ONLINE : (current?.id ?? null));
  const informational = job === "aloud";
  const chosen = choices?.find((c) => c.id === selected);
  const canUse = chosen !== undefined && !chosen.current && !informational;

  const here =
    choices?.filter((c) => (c.current || c.onDisk) && c.fit !== "wontFit" && c.fit !== "cantUse") ??
    [];
  const get =
    choices?.filter((c) => !c.current && !c.onDisk && c.fit !== "wontFit" && c.fit !== "cantUse") ??
    [];
  const cant =
    choices?.filter((c) => !c.current && (c.fit === "wontFit" || c.fit === "cantUse")) ?? [];

  const ask = async () => {
    if (!chosen) return;
    setProblem(false);
    try {
      setPreview(await engine.previewModelChange(job, chosen.id));
    } catch {
      setProblem(true);
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
        <p className={styles.body} role="alert">
          {t("models.pick.error")}
        </p>
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
                      informational={informational}
                      onPick={() => setPicked(c.id)}
                    />
                  ))}
                </Group>
              ))}
            {job === "answering" ? (
              <Group title={t("models.group.online")}>
                <OnlineOption
                  checked={selected === ONLINE}
                  onPick={() => setPicked(ONLINE)}
                  answeringName={current?.name ?? ""}
                />
              </Group>
            ) : null}
          </fieldset>
          {informational ? (
            <p className={styles.body}>{t("models.aloud.only", { name: current?.name ?? "" })}</p>
          ) : null}
          {problem ? (
            <p className={styles.body} role="alert">
              {t("models.confirm.failed")}
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

      {preview && chosen ? (
        <Confirm
          preview={preview}
          name={chosen.name}
          fromName={current?.name ?? preview.from}
          onClose={() => setPreview(null)}
          onConfirm={async (allowDownload) => {
            await change(job, chosen.id, allowDownload);
            setPreview(null);
            window.location.hash = "#/settings/models";
          }}
        />
      ) : null}
    </section>
  );
}
