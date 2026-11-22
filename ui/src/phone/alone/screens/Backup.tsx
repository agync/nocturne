import { useEffect, useId, useRef, useState } from "react";
import { formatBytes, formatLongDay, formatTime12 } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BackupProgress, BackupReport, KeptCounts } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import { type Checked, HomeRequestError, type PickedFile, type Progress } from "../api";
import { chips } from "./parts";

const MIN_PASSPHRASE = 8;

function ModalBar({ title, onClose }: { title: string; onClose(): void }) {
  const heading = useHeading(title);
  return (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onClose}
        aria-label={t("phone.cancel")}
      >
        <Icon name="close" />
      </button>
      <h1 className="ph-bar-title ph-serif" tabIndex={-1} ref={heading}>
        {title}
      </h1>
    </header>
  );
}

function BackOnly({ onBack }: { onBack(): void }) {
  return (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onBack}
        aria-label={t("restore.back")}
      >
        <Icon name="back" />
      </button>
    </header>
  );
}

/** A backup or a restore running: its phase, and how far it is. */
export function Running({ title, progress }: { title: string; progress: BackupProgress | null }) {
  const heading = useHeading(title);
  const share = progress && progress.total > 0 ? progress.done / progress.total : 0;
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy="true">
        <div className="ph-hero" style={{ paddingTop: 72 }}>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {title}
          </h1>
        </div>
        <div
          className="ph-progress"
          role="progressbar"
          aria-label={title}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(share * 100)}
        >
          <span style={{ width: `${Math.round(share * 100)}%` }} />
        </div>
        {progress && progress.total > 0 ? (
          <p className="ph-small">
            {t("backup.progress.plain", {
              done: formatBytes(progress.done),
              total: formatBytes(progress.total),
            })}
          </p>
        ) : null}
      </main>
    </Page>
  );
}

/** "Your backup is saved." or "Your memory is back on this phone.", and the one button on. */
export function Done({
  title,
  body,
  button,
  onDone,
}: {
  title: string;
  body: string;
  button: string;
  onDone(): void;
}) {
  const heading = useHeading(title);
  return (
    <Page bar={null}>
      <main className="ph-main">
        <div className="ph-hero" style={{ paddingTop: 72 }} role="status">
          <span className="ph-hero-icon">
            <Icon name="check" size={40} />
          </span>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {title}
          </h1>
          <p className="ph-body ph-ink2">{body}</p>
        </div>
        <div className="ph-spacer" />
        <Button kind="primary" onClick={onDone}>
          {button}
        </Button>
      </main>
    </Page>
  );
}

function Passphrase({
  label,
  value,
  shown,
  hint,
  onChange,
}: {
  label: string;
  value: string;
  shown: boolean;
  hint?: string;
  onChange(next: string): void;
}) {
  const id = useId();
  return (
    <div className="ph-field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        className="ph-input"
        type={shown ? "text" : "password"}
        autoComplete="new-password"
        autoCapitalize="none"
        spellCheck={false}
        value={value}
        aria-describedby={hint ? `${id}-hint` : undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {hint ? (
        <p className="ph-small" id={`${id}-hint`}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}

type SaveStep =
  | { name: "form"; error: "mismatch" | "failed" | "full" | null }
  | { name: "saving"; progress: BackupProgress | null }
  | { name: "saved"; report: BackupReport; at: Date };

/**
 * Save a backup: 0013's passphrase form, then the backup is made and the phone's own save sheet
 * asks where it goes. A sheet closed without choosing changes nothing.
 */
export function SaveBackup({
  save,
  now,
  onClose,
}: {
  /** Makes the backup, then the save sheet; null when the person chose nowhere. */
  save(passphrase: string, onProgress: Progress): Promise<BackupReport | null>;
  now(): number;
  onClose(): void;
}) {
  const [step, setStep] = useState<SaveStep>({ name: "form", error: null });
  const [first, setFirst] = useState("");
  const [again, setAgain] = useState("");
  const [shown, setShown] = useState(false);
  const [ticked, setTicked] = useState(false);
  const tickId = useId();

  const ready = first.length >= MIN_PASSPHRASE && again.length > 0 && ticked;

  const go = async () => {
    if (!ready) return;
    if (first !== again) return setStep({ name: "form", error: "mismatch" });
    setStep({ name: "saving", progress: null });
    try {
      const report = await save(first, (progress) =>
        setStep((s) => (s.name === "saving" ? { name: "saving", progress } : s)),
      );
      setStep(
        report ? { name: "saved", report, at: new Date(now()) } : { name: "form", error: null },
      );
    } catch (e) {
      const full = e instanceof HomeRequestError && e.code === "full";
      setStep({ name: "form", error: full ? "full" : "failed" });
    }
  };

  if (step.name === "saving")
    return (
      <Running
        title={
          step.progress?.phase === "checking"
            ? t("backup.phase.checking")
            : t("backup.making.title")
        }
        progress={step.progress}
      />
    );
  if (step.name === "saved")
    return (
      <Done
        title={t("backup.saved.title")}
        body={t("phone.alone.backup.made", {
          size: formatBytes(step.report.bytes),
          time: formatTime12(step.at),
        })}
        button={t("phone.sources.ok")}
        onDone={onClose}
      />
    );

  return (
    <Page bar={<ModalBar title={t("phone.alone.backup.title")} onClose={onClose} />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <p className="ph-body ph-ink2">{t("phone.alone.backup.lead")}</p>
        <Passphrase
          label={t("backup.field.label")}
          value={first}
          shown={shown}
          hint={t("backup.field.hint")}
          onChange={setFirst}
        />
        <Passphrase
          label={t("backup.field.again")}
          value={again}
          shown={shown}
          onChange={setAgain}
        />
        <button
          type="button"
          className="ph-link"
          aria-pressed={shown}
          onClick={() => setShown((s) => !s)}
        >
          {t("backup.show")}
        </button>
        {step.error === "mismatch" ? (
          <p className="ph-error" role="alert">
            {t("backup.error.mismatch")}
          </p>
        ) : null}
        <div className="ph-limit ph-stack" style={{ gap: 8 }}>
          <strong style={{ fontWeight: 500 }}>{t("backup.note.title")}</strong>
          <span>{t("backup.note.body")}</span>
        </div>
        <label className="ph-check" htmlFor={tickId}>
          <input
            id={tickId}
            type="checkbox"
            checked={ticked}
            onChange={(e) => setTicked(e.target.checked)}
          />
          {t("backup.note.check")}
        </label>
        {step.error === "failed" || step.error === "full" ? (
          <div className="ph-stack" role="alert" style={{ gap: 4 }}>
            <p className="ph-card-title">{t("backup.fail.stopped")}</p>
            <p className="ph-small">
              {step.error === "full" ? t("backup.fail.room.body") : t("backup.fail.other.body")}
            </p>
          </div>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={!ready} onClick={() => void go()}>
          {t("phone.alone.backup.save")}
        </Button>
        <p className="ph-bar-line">{t("phone.alone.backup.next")}</p>
      </footer>
    </Page>
  );
}

type RestoreError = "wrong" | "bad" | "newer" | "damaged" | "full" | "failed";

type RestoreStep =
  | { name: "form"; error: RestoreError | null }
  | { name: "replace"; checked: Checked }
  | { name: "running"; phase: "checking" | "restoring"; progress: BackupProgress | null }
  | { name: "done"; count: number };

/**
 * Bring back your memory: the phone's open sheet for the backup file, its passphrase, then 0013's
 * checks. The chosen file's copy goes when the person leaves without bringing it back.
 */
export function Restore({
  pickFile,
  check,
  restore,
  letGo,
  onBack,
  onDone,
}: {
  /** The phone's open sheet; null when closed, a `notABackup` error for another file. */
  pickFile(): Promise<PickedFile | null>;
  check(file: string, passphrase: string, onProgress: Progress): Promise<Checked>;
  restore(file: string, passphrase: string, replace: boolean, onProgress: Progress): Promise<void>;
  letGo(): Promise<void>;
  onBack(): void;
  onDone(): void;
}) {
  const [file, setFile] = useState<PickedFile | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [step, setStep] = useState<RestoreStep>({ name: "form", error: null });
  const heading = useHeading("restore");
  const kept = useRef(false);

  useEffect(
    () => () => {
      if (!kept.current) void letGo();
    },
    [letGo],
  );

  const choose = async () => {
    try {
      const picked = await pickFile();
      if (picked) {
        setFile(picked);
        setStep({ name: "form", error: null });
      }
    } catch (e) {
      setFile(null);
      setStep({ name: "form", error: restoreError(e) });
    }
  };

  const running =
    (phase: "checking" | "restoring") =>
    (progress: BackupProgress): void =>
      setStep({ name: "running", phase, progress });

  const failed = (e: unknown) => setStep({ name: "form", error: restoreError(e) });

  const bringIn = async (checked: Checked, replace: boolean) => {
    if (!file) return;
    try {
      setStep({ name: "running", phase: "restoring", progress: null });
      await restore(file.path, passphrase, replace, running("restoring"));
      kept.current = true;
      setStep({ name: "done", count: checked.backup.things });
    } catch (e) {
      failed(e);
    }
  };

  const bring = async () => {
    if (!file || passphrase === "") return;
    try {
      setStep({ name: "running", phase: "checking", progress: null });
      const checked = await check(file.path, passphrase, running("checking"));
      if (checked.check.here !== null) return setStep({ name: "replace", checked });
      await bringIn(checked, false);
    } catch (e) {
      failed(e);
    }
  };

  if (step.name === "running")
    return (
      <Running
        title={
          step.phase === "checking" ? t("restore.checking.title") : t("restore.restoring.title")
        }
        progress={step.progress}
      />
    );
  if (step.name === "replace")
    return (
      <Replace
        checked={step.checked}
        onKeep={onBack}
        onReplace={() => void bringIn(step.checked, true)}
      />
    );
  if (step.name === "done")
    return (
      <Done
        title={t("phone.alone.restore.done.title")}
        body={t("phone.alone.restore.done.body", { count: step.count })}
        button={t("restore.restored.continue")}
        onDone={onDone}
      />
    );

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.restore.title")}
        </h1>
        <p className="ph-body ph-ink2">{t("phone.alone.restore.lead")}</p>
        {file ? (
          <div className="ph-filecard">
            <Icon name="document" size={22} />
            <span className="ph-srow-text">
              <span className="ph-srow-label">{file.name}</span>
              <span className="ph-srow-hint">{formatBytes(file.bytes)}</span>
            </span>
            <button type="button" className="ph-link" onClick={() => void choose()}>
              {t("phone.alone.restore.change")}
            </button>
          </div>
        ) : null}
        {file ? (
          <Passphrase
            label={t("restore.field.label")}
            value={passphrase}
            shown={false}
            onChange={setPassphrase}
          />
        ) : null}
        {step.error ? <RestoreProblem error={step.error} /> : null}
      </main>
      <footer className="ph-foot">
        {file ? (
          <Button kind="primary" disabled={passphrase === ""} onClick={() => void bring()}>
            {t("phone.alone.restore.bring")}
          </Button>
        ) : (
          <Button kind="primary" onClick={() => void choose()}>
            {t("phone.alone.restore.choose")}
          </Button>
        )}
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("restore.back")}
        </button>
      </footer>
    </Page>
  );
}

function restoreError(e: unknown): RestoreError {
  const code = e instanceof HomeRequestError ? e.code : "failed";
  switch (code) {
    case "wrongPassphrase":
      return "wrong";
    case "notABackup":
      return "bad";
    case "newerBackup":
      return "newer";
    case "damagedBackup":
      return "damaged";
    case "full":
      return "full";
    default:
      return "failed";
  }
}

/** 0013's messages for a backup that did not come back. */
function RestoreProblem({ error }: { error: RestoreError }) {
  const [title, body] =
    error === "bad"
      ? [t("restore.bad.title"), t("restore.bad.body")]
      : error === "newer"
        ? [t("restore.newer.title"), t("phone.alone.restore.newer.body")]
        : error === "damaged"
          ? [t("restore.damaged.title"), t("restore.damaged.body")]
          : error === "full"
            ? [t("restore.noRoom.title"), t("phone.alone.restore.noRoom.body")]
            : [null, error === "wrong" ? t("restore.wrong") : t("setup.action.failed")];
  return (
    <div className="ph-stack" role="alert" style={{ gap: 4 }}>
      {title ? <p className="ph-card-title">{title}</p> : null}
      {body ? <p className={title ? "ph-small" : "ph-error"}>{body}</p> : null}
    </div>
  );
}

function Count({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="ph-replace-row">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function Columns({ counts }: { counts: KeptCounts }) {
  return (
    <>
      <Count label={t("phone.alone.replace.things")} value={counts.things} />
      <Count label={t("phone.alone.replace.journal")} value={counts.journalEntries} />
      <Count label={t("phone.alone.replace.originals")} value={counts.photosAndRecordings} />
    </>
  );
}

/** "Replace what's on this phone?": 0013's choice, worded for the phone; neither is the default. */
function Replace({
  checked,
  onKeep,
  onReplace,
}: {
  checked: Checked;
  onKeep(): void;
  onReplace(): void;
}) {
  const heading = useHeading("replace");
  const { summary, here, putAside } = checked.check;
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.replace.title")}
        </h1>
        <p className="ph-body ph-ink2">{chips("phone.alone.replace.body")}</p>
        <section className="ph-replace" aria-labelledby="ph-replace-in">
          <h2 className="ph-see-head" id="ph-replace-in">
            {t("restore.confirm.in")} · {t("restore.confirm.in.note")}
          </h2>
          <dl>
            <Count label={t("restore.confirm.made")} value={formatLongDay(summary.madeAt)} />
            <Columns counts={checked.backup} />
          </dl>
        </section>
        {here ? (
          <section className="ph-replace" aria-labelledby="ph-replace-here">
            <h2 className="ph-see-head" id="ph-replace-here">
              {t("phone.alone.replace.here")} · {t("restore.confirm.here.note")}
            </h2>
            <dl>
              {here.lastKept ? (
                <Count label={t("restore.confirm.lastKept")} value={formatLongDay(here.lastKept)} />
              ) : null}
              {checked.here ? <Columns counts={checked.here} /> : null}
              {here.keptAfterBackup > 0 ? (
                <Count label={t("restore.confirm.keptAfter")} value={here.keptAfterBackup} />
              ) : null}
            </dl>
          </section>
        ) : null}
        {putAside ? (
          <p className="ph-small">
            {t("restore.confirm.already", { day: formatLongDay(putAside) })}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button onClick={onKeep}>{t("restore.confirm.keep")}</Button>
        <Button onClick={onReplace}>{t("restore.confirm.replace")}</Button>
      </footer>
    </Page>
  );
}
