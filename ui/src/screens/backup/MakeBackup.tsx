import { useQuery } from "@tanstack/react-query";
import { type RefObject, useEffect, useId, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Checkbox } from "../../components/Checkbox/Checkbox";
import { tc } from "../../components/ControlChip/copy";
import { Chevron } from "../../components/Disclosure/Chevron";
import { Icon } from "../../components/Icon/Icon";
import { Progress } from "../../components/Progress/Progress";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { TextInput } from "../../components/TextInput/TextInput";
import { formatBytes } from "../../copy/format";
import { t } from "../../copy/t";
import type { BackupProgress, BackupReport } from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine, useEngineEvent, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import styles from "./Backup.module.css";
import { fileNameOf, folderOf } from "./paths";
import { progressDetail, useProgressEta } from "./progress";
import { activeBackup, clearBackup, startBackup, useBackupAttempt } from "./run";
import { lengthOf, type Strength, strengthOf } from "./strength";

type Step = "form" | "running" | "saved";
type Failure = "room" | "cantWrite" | "notFound" | "damagedBackup" | "other";

const failureOf = (code: string): Failure => {
  switch (code) {
    case "noRoom":
      return "room";
    case "cantWrite":
      return "cantWrite";
    case "notFound":
      return "notFound";
    case "damagedBackup":
      return "damagedBackup";
    default:
      return "other";
  }
};

/** Serves #/privacy/backup: one file, locked with a passphrase the person chooses. */
export default function MakeBackup() {
  const engine = useEngine();
  const platform = usePlatform();
  const refresh = useRefresh();
  const initial = useRef(activeBackup()).current;
  const attempt = useBackupAttempt();

  const [step, setStep] = useState<Step>(initial && !initial.outcome ? "running" : "form");
  const [passphrase, setPassphrase] = useState(initial?.passphrase ?? "");
  const [again, setAgain] = useState(initial?.passphrase ?? "");
  const [show, setShow] = useState(false);
  const [safe, setSafe] = useState(initial !== null);
  const [mismatch, setMismatch] = useState("");
  const [spoken, setSpoken] = useState<Strength | null>(
    initial?.passphrase ? strengthOf(initial.passphrase) : null,
  );
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [path, setPath] = useState<string | null>(initial?.path ?? null);
  const [report, setReport] = useState<BackupReport | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [stopped, setStopped] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [advanced, setAdvanced] = useState(false);

  const titleRef = useRef<HTMLHeadingElement>(null);
  const resultRef = useRef<HTMLDivElement>(null);
  const firstStep = useRef(true);
  const adopted = useRef(false);
  const sawRunning = useRef(initial !== null);
  const lastPhase = useRef<BackupProgress["phase"] | null>(null);
  const hintId = useId();
  const advancedId = useId();

  const estimate = useQuery({
    queryKey: ["backupEstimate"],
    queryFn: () => engine.estimateBackup(),
  });
  const backups = useQuery({
    queryKey: ["backups"],
    queryFn: () => engine.getBackups(),
    staleTime: 0,
    refetchInterval: (query) => (query.state.data?.running ? 1000 : false),
  });
  const privacy = useQuery({
    queryKey: ["privacy"],
    queryFn: () => engine.getPrivacy(),
    staleTime: 0,
  });
  const remote = useQuery({
    queryKey: ["remote"],
    queryFn: () => engine.getRemote(),
    staleTime: 0,
  });
  const counts = useQuery({
    queryKey: ["backupCounts"],
    enabled: step === "running",
    staleTime: 0,
    queryFn: async () => {
      const [memories, pages, items] = await Promise.all([
        engine.listMemories({
          entityId: null,
          itemId: null,
          status: null,
          query: null,
          limit: null,
        }),
        engine.listPages(),
        engine.listItems({ since: null, status: null, limit: null }),
      ]);
      return {
        memories: memories.length,
        pages: pages.length,
        originals: items.filter((item) => item.hasOriginal).length,
      };
    },
  });

  // A run that started before this screen was opened shows by its progress, and follows the
  // same events as one started here.
  useEngineEvent("backup-progress", (next) => {
    if (next.phase === "restoring") return;
    sawRunning.current = true;
    setProgress(next);
    setStep((current) => (current === "saved" ? current : "running"));
  });

  useEffect(() => {
    if (adopted.current || backups.data === undefined) return;
    adopted.current = true;
    const running = backups.data.running;
    if (attempt || !running || running.phase === "restoring") return;
    sawRunning.current = true;
    setStep("running");
    setProgress(running);
    lastPhase.current = running.phase;
    setAnnouncement(t("backup.announce.start"));
  }, [backups.data, attempt]);

  // A run this window cannot follow to its report (a reopened window) gives the form back when
  // it is no longer running.
  useEffect(() => {
    if (step !== "running" || attempt || !sawRunning.current) return;
    if (backups.data === undefined || backups.data.running !== null) return;
    setStep("form");
    setProgress(null);
    setAnnouncement("");
    refresh("backups", "privacy");
  }, [step, attempt, backups.data, refresh]);

  useEffect(() => {
    if (!attempt?.outcome) return;
    if ("report" in attempt.outcome) {
      setReport(attempt.outcome.report);
      setProgress(null);
      setStep("saved");
      setAnnouncement(t("backup.announce.saved"));
    } else {
      const code = toEngineError(attempt.outcome.error).code;
      setProgress(null);
      setAnnouncement("");
      setStopped(code === "cancelled");
      setFailure(code === "cancelled" ? null : failureOf(code));
      setStep("form");
    }
    refresh("backups", "privacy", "backupEstimate");
    clearBackup();
  }, [attempt, refresh]);

  useEffect(() => {
    if (firstStep.current) {
      firstStep.current = false;
      return;
    }
    titleRef.current?.focus({ preventScroll: true });
  }, [step]);

  useEffect(() => {
    if (failure) resultRef.current?.querySelector("button")?.focus({ preventScroll: true });
  }, [failure]);

  // A run that is over is forgotten with the screen, and its passphrase with it. One still
  // running stays in memory so the person can come back to it.
  useEffect(
    () => () => {
      if (activeBackup()?.outcome) clearBackup();
    },
    [],
  );

  useEffect(() => {
    if (passphrase === "") {
      setSpoken(null);
      return;
    }
    const timer = setTimeout(() => setSpoken(strengthOf(passphrase)), 600);
    return () => clearTimeout(timer);
  }, [passphrase]);

  const phase = progress?.phase ?? null;
  useEffect(() => {
    if (phase === lastPhase.current) return;
    if (phase === "checking") setAnnouncement(t("backup.announce.checking"));
    lastPhase.current = phase;
  }, [phase]);

  const eta = useProgressEta(progress);
  const ready = safe && lengthOf(passphrase) >= 8 && again === passphrase;

  const begin = (chosen: string) => {
    setPath(chosen);
    setFailure(null);
    setStopped(false);
    setReport(null);
    setProgress(null);
    setMismatch("");
    setStep("running");
    setAnnouncement(t("backup.announce.start"));
    lastPhase.current = null;
    sawRunning.current = true;
    startBackup(engine, chosen, passphrase);
  };

  const save = async () => {
    if (!ready) return;
    let chosen: string | null;
    try {
      chosen = await platform.pickBackupSave();
    } catch {
      setFailure("other");
      return;
    }
    if (chosen === null) return;
    begin(chosen);
  };

  const chooseAnother = async () => {
    let chosen: string | null;
    try {
      chosen = await platform.pickBackupSave();
    } catch {
      setFailure("other");
      return;
    }
    if (chosen !== null) begin(chosen);
  };

  const tryAgain = () => {
    if (path === null) {
      void chooseAnother();
      return;
    }
    begin(path);
  };

  const stop = () => {
    void engine.cancelBackup().catch(() => undefined);
  };

  const done = () => {
    clearBackup();
    refresh("backups", "privacy");
    goTo("/privacy");
  };

  const cancel = () => {
    clearBackup();
    goTo("/privacy");
  };

  const onChangeAgain = (next: string) => {
    setAgain(next);
    if (mismatch && next === passphrase) setMismatch("");
  };

  const onBlurAgain = () => {
    if (again !== "" && again !== passphrase) setMismatch(t("backup.error.mismatch"));
  };

  const title =
    step === "running"
      ? t("backup.making.title")
      : step === "saved"
        ? t("backup.saved.title")
        : t("backup.title");

  // The marks follow every keystroke; only the words wait for the pause and the live region.
  const current = passphrase === "" ? null : strengthOf(passphrase);
  const onlineSetUp = privacy.data?.onlineHost != null;
  const phonesAdded = (remote.data?.devices.length ?? 0) > 0;
  const estimateText =
    typeof estimate.data === "number"
      ? t("backup.hint.ready", { size: formatBytes(estimate.data) })
      : null;

  return (
    <ScreenSheet>
      <BackLink to="/privacy" label={t("screen.privacy.title")} />
      <div className={styles.columns}>
        <section className={styles.main}>
          <p className="sr-only" role="status" aria-live="polite">
            {announcement}
          </p>
          <h1 ref={titleRef} tabIndex={-1} className={styles.stepTitle}>
            {title}
          </h1>

          {step === "form" ? (
            <>
              <p className={styles.lead}>{t("backup.lead")}</p>
              <div className={styles.fields}>
                <div>
                  <TextInput
                    label={t("backup.field.label")}
                    hint={t("backup.field.hint")}
                    value={passphrase}
                    type={show ? "text" : "password"}
                    autoComplete="new-password"
                    spellCheck={false}
                    onChange={setPassphrase}
                  />
                  <p className={styles.strength}>
                    {current ? (
                      <span className={styles.marks} data-level={current.marks} aria-hidden="true">
                        <span />
                        <span />
                        <span />
                      </span>
                    ) : null}
                    <span aria-live="polite">{spoken ? t(spoken.copy) : ""}</span>
                  </p>
                </div>
                <TextInput
                  label={t("backup.field.again")}
                  value={again}
                  type={show ? "text" : "password"}
                  autoComplete="new-password"
                  spellCheck={false}
                  error={mismatch || undefined}
                  onChange={onChangeAgain}
                  onBlur={onBlurAgain}
                />
                <Checkbox label={t("backup.show")} checked={show} onChange={() => setShow(!show)} />
              </div>

              <div className={styles.note}>
                <Icon name="lock" size={20} />
                <div className={styles.noteText}>
                  <p className={styles.noteTitle}>{t("backup.note.title")}</p>
                  <p>{t("backup.note.body")}</p>
                  <Checkbox
                    label={t("backup.note.check")}
                    checked={safe}
                    onChange={() => setSafe(!safe)}
                  />
                </div>
              </div>

              {stopped ? (
                <div className={styles.result} role="status">
                  <p className={styles.resultTitle}>{t("backup.stopped.title")}</p>
                  <p>{t("backup.stopped.body")}</p>
                </div>
              ) : null}

              {failure ? (
                <FailureBox
                  failure={failure}
                  boxRef={resultRef}
                  onTryAgain={tryAgain}
                  onChooseAnother={() => void chooseAnother()}
                />
              ) : (
                <div className={styles.footer}>
                  <Button
                    variant="primary"
                    disabled={!ready}
                    aria-describedby={ready ? undefined : hintId}
                    onClick={() => void save()}
                  >
                    {t("backup.save")}
                  </Button>
                  <Button variant="quiet" onClick={cancel}>
                    {t("dialog.cancel")}
                  </Button>
                  <span id={hintId} className={styles.footerHint}>
                    {ready ? (estimateText ?? "") : t("backup.hint.notReady")}
                  </span>
                </div>
              )}

              <Advanced
                id={advancedId}
                open={advanced}
                onToggle={() => setAdvanced(!advanced)}
                path={null}
                dir={privacy.data?.dataDir ?? null}
              />
            </>
          ) : null}

          {step === "running" ? (
            <>
              <Progress
                label={t(phase === "checking" ? "backup.phase.checking" : "backup.phase.saving")}
                value={progress?.done ?? 0}
                max={progress?.total ?? 0}
                detail={progress ? progressDetail(progress, eta) : undefined}
              />
              {path ? <p className={styles.mono}>{t("backup.savingTo", { path })}</p> : null}
              <p className={styles.lead}>{t("backup.making.body")}</p>
              <p className={styles.small}>{t("backup.making.drive")}</p>
              <div className={styles.footer}>
                <Button variant="secondary" onClick={stop}>
                  {t("backup.stop")}
                </Button>
              </div>
            </>
          ) : null}

          {step === "saved" && report ? (
            <>
              <dl className={styles.receipt}>
                <div>
                  <dt>{t("backup.saved.file")}</dt>
                  <dd className={styles.fileName}>{fileNameOf(report.path)}</dd>
                </div>
                <div>
                  <dt>{t("backup.saved.folder")}</dt>
                  <dd className={styles.mono}>{folderOf(report.path)}</dd>
                </div>
                <div>
                  <dt>{t("backup.saved.holds.label")}</dt>
                  <dd>
                    {t("backup.saved.holds", {
                      size: formatBytes(report.bytes),
                      memories: report.memories,
                      pages: report.pages,
                      originals: report.originals,
                    })}
                  </dd>
                </div>
                <div>
                  <dt>{t("backup.saved.checked.label")}</dt>
                  <dd className={styles.ticked}>{t("backup.saved.checked")}</dd>
                </div>
              </dl>
              <p className={styles.lead}>{tc("backup.saved.body")}</p>
              <div className={styles.footer}>
                <Button
                  variant="secondary"
                  onClick={() => void platform.openPath(folderOf(report.path))}
                >
                  {t("backup.saved.show")}
                </Button>
                <Button variant="primary" onClick={done}>
                  {t("backup.saved.done")}
                </Button>
              </div>
              <Advanced
                id={advancedId}
                open={advanced}
                onToggle={() => setAdvanced(!advanced)}
                path={report.path}
                dir={privacy.data?.dataDir ?? null}
              />
            </>
          ) : null}
        </section>

        {step === "form" ? (
          <aside className={styles.aside}>
            <h2 className={styles.label}>{t("backup.aside.in")}</h2>
            <ul className={styles.rules}>
              <li>{t("backup.aside.in.kept")}</li>
              <li>{t("backup.aside.in.remembers")}</li>
              <li>{t("backup.aside.in.pages")}</li>
              <li>{t("backup.aside.in.settings")}</li>
              <li>{t("backup.aside.in.private")}</li>
            </ul>
            <h2 className={styles.label}>{t("backup.aside.out")}</h2>
            <ul className={`${styles.rules} ${styles.out}`}>
              <li>
                <strong>{t("backup.aside.out.mind.title")}</strong>{" "}
                {t("backup.aside.out.mind.body")}
              </li>
              {onlineSetUp ? (
                <li>
                  <strong>{t("backup.aside.out.key.title")}</strong>{" "}
                  {t("backup.aside.out.key.body")}
                </li>
              ) : null}
              {phonesAdded ? (
                <li>
                  <strong>{t("backup.aside.out.phones.title")}</strong>{" "}
                  {tc("backup.aside.out.phones.body")}
                </li>
              ) : null}
            </ul>
          </aside>
        ) : null}

        {step === "running" ? (
          <aside className={styles.aside}>
            <h2 className={styles.label}>{t("backup.aside.going")}</h2>
            <ul className={styles.rules}>
              {counts.data ? (
                <>
                  <li>{t("backup.aside.memories", { count: counts.data.memories })}</li>
                  <li>{t("backup.aside.pages", { count: counts.data.pages })}</li>
                  <li>
                    {t("backup.aside.originals", { count: counts.data.originals })}
                    <small>{t("backup.aside.originals.note")}</small>
                  </li>
                </>
              ) : null}
              <li>{t("backup.aside.settings")}</li>
            </ul>
          </aside>
        ) : null}
      </div>
    </ScreenSheet>
  );
}

function FailureBox({
  failure,
  boxRef,
  onTryAgain,
  onChooseAnother,
}: {
  failure: Failure;
  boxRef: RefObject<HTMLDivElement | null>;
  onTryAgain: () => void;
  onChooseAnother: () => void;
}) {
  const title =
    failure === "cantWrite"
      ? t("backup.fail.cantWrite.title")
      : failure === "damagedBackup"
        ? t("backup.fail.check.title")
        : t("backup.fail.stopped");
  const body =
    failure === "room"
      ? t("backup.fail.room.body")
      : failure === "cantWrite"
        ? t("backup.fail.cantWrite.body")
        : failure === "notFound"
          ? t("backup.fail.off.body")
          : failure === "damagedBackup"
            ? t("backup.fail.check.body")
            : t("backup.fail.other.body");
  return (
    <div ref={boxRef} className={styles.result} data-kind="bad" role="alert">
      <p className={styles.resultTitle}>{title}</p>
      <p>{body}</p>
      <div className={styles.buttons}>
        {failure === "room" || failure === "cantWrite" ? null : (
          <Button variant="secondary" onClick={onTryAgain}>
            {t("button.tryAgain")}
          </Button>
        )}
        {failure === "room" || failure === "cantWrite" || failure === "notFound" ? (
          <Button variant="quiet" onClick={onChooseAnother}>
            {t("backup.fail.place")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function Advanced({
  id,
  open,
  onToggle,
  path,
  dir,
}: {
  id: string;
  open: boolean;
  onToggle: () => void;
  path: string | null;
  dir: string | null;
}) {
  const rows: Array<[string, string]> = [
    [
      t("advanced.backup.file"),
      path
        ? t("advanced.backup.file.value", {
            file: fileNameOf(path),
            partial: `${fileNameOf(path)}.partial`,
          })
        : t("advanced.backup.file.pending"),
    ],
    [t("advanced.backup.locking"), t("advanced.backup.locking.value")],
    [t("advanced.backup.leftOut"), t("advanced.backup.leftOut.value")],
  ];
  if (dir !== null) {
    rows.push([t("advanced.backup.putAside"), t("advanced.backup.putAside.value", { dir })]);
  }
  return (
    <div className={styles.more}>
      <button
        type="button"
        className={`${styles.moreButton} hit`}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={onToggle}
      >
        <Chevron />
        {t("advanced.title")}
      </button>
      {open ? (
        <dl id={id} className={styles.advanced}>
          {rows.map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </div>
  );
}
