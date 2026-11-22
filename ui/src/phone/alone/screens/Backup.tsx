import { useId, useState } from "react";
import { formatBytes, formatTime12 } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BackupProgress, BackupReport, RestoreCheck } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import { HomeRequestError, type PickedFile, type Progress } from "../api";

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
function Running({ title, progress }: { title: string; progress: BackupProgress | null }) {
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
function Done({
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

/** Save a backup: 0013's passphrase form, the phone's own save sheet, then the result. */
export function SaveBackup({
  save,
  pickDest,
  now,
  onClose,
}: {
  save(dest: string, passphrase: string, onProgress: Progress): Promise<BackupReport>;
  /** The phone's save sheet; null when the person chose nowhere. */
  pickDest(suggested: string): Promise<string | null>;
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
    const day = new Date(now()).toISOString().slice(0, 10);
    const dest = await pickDest(`nocturne-backup-${day}.nocturne`);
    if (!dest) return;
    setStep({ name: "saving", progress: null });
    try {
      const report = await save(dest, first, (progress) =>
        setStep((s) => (s.name === "saving" ? { name: "saving", progress } : s)),
      );
      setStep({ name: "saved", report, at: new Date(now()) });
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

type RestoreStep =
  | { name: "form"; error: "wrong" | "bad" | "full" | "failed" | "here" | null }
  | { name: "running"; phase: "checking" | "restoring"; progress: BackupProgress | null }
  | { name: "done"; count: number };

/** Bring back your memory: choose the backup file, type its passphrase, then 0013's checks. */
export function Restore({
  pickFile,
  check,
  restore,
  onBack,
  onDone,
}: {
  pickFile(): Promise<PickedFile | null>;
  check(file: string, passphrase: string, onProgress: Progress): Promise<RestoreCheck>;
  restore(file: string, passphrase: string, replace: boolean, onProgress: Progress): Promise<void>;
  onBack(): void;
  onDone(): void;
}) {
  const [file, setFile] = useState<PickedFile | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [step, setStep] = useState<RestoreStep>({ name: "form", error: null });
  const heading = useHeading("restore");

  const choose = async () => {
    const picked = await pickFile();
    if (picked) {
      setFile(picked);
      setStep({ name: "form", error: null });
    }
  };

  const bring = async () => {
    if (!file || passphrase === "") return;
    const running =
      (phase: "checking" | "restoring") =>
      (progress: BackupProgress): void =>
        setStep({ name: "running", phase, progress });
    try {
      setStep({ name: "running", phase: "checking", progress: null });
      const checked = await check(file.path, passphrase, running("checking"));
      // What is here is never replaced without asking, and the phone's question is not written yet.
      if (checked.here !== null) return setStep({ name: "form", error: "here" });
      setStep({ name: "running", phase: "restoring", progress: null });
      await restore(file.path, passphrase, false, running("restoring"));
      setStep({ name: "done", count: checked.summary.items });
    } catch (e) {
      const code = e instanceof HomeRequestError ? e.code : "failed";
      const error =
        code === "wrongPassphrase"
          ? "wrong"
          : code === "notABackup"
            ? "bad"
            : code === "full"
              ? "full"
              : "failed";
      setStep({ name: "form", error });
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
        {step.error ? (
          <div className="ph-stack" role="alert" style={{ gap: 4 }}>
            {step.error === "bad" ? (
              <>
                <p className="ph-card-title">{t("restore.bad.title")}</p>
                <p className="ph-small">{t("restore.bad.body")}</p>
              </>
            ) : (
              <p className="ph-error">
                {step.error === "wrong"
                  ? t("restore.wrong")
                  : step.error === "full"
                    ? t("phone.alone.full")
                    : t("setup.action.failed")}
              </p>
            )}
          </div>
        ) : null}
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
