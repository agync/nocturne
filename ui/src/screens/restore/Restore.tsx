import { useQuery } from "@tanstack/react-query";
import { type RefObject, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import packageJson from "../../../package.json";
import { goTo } from "../../app/query";
import { ActionBar, ActionBarHost } from "../../components/ActionBar/ActionBar";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Checkbox } from "../../components/Checkbox/Checkbox";
import { tc } from "../../components/ControlChip/copy";
import { Chevron } from "../../components/Disclosure/Chevron";
import { Progress } from "../../components/Progress/Progress";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { TextInput } from "../../components/TextInput/TextInput";
import { formatBytes, formatDayMonth, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { BackupProgress, RestoreCheck, RestoredView } from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine, useEngineEvent, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import layout from "../backup/Backup.module.css";
import { fileNameOf } from "../backup/paths";
import { progressDetail, useProgressEta } from "../backup/progress";
import styles from "./Restore.module.css";

export type RestoreMode = "privacy" | "setup";

export interface RestoreFlowProps {
  mode?: RestoreMode;
  /** Leaving before a restore begins. Privacy goes back on its own; setup calls this. */
  onExit?: () => void;
  /** The primary button of the last step in setup, with what the backup brought in. */
  onContinue?: (restored: RestoredView) => void;
}

type Step = "choose" | "passphrase" | "checking" | "ready" | "confirm" | "restoring" | "restored";

type FailureKind = "bad" | "newer" | "damaged" | "gone" | "noRoom";

interface Failure {
  kind: FailureKind;
  theirs?: string | null;
}

const APP_VERSION = packageJson.version;

const versionIn = (detail: string): string | null => /\b\d+\.\d+\.\d+\b/.exec(detail)?.[0] ?? null;

const newerFailure = (detail: string): Failure => ({ kind: "newer", theirs: versionIn(detail) });

/** The failures a picked file or a check can end with, in the words of their own boxes. */
function failureFrom(code: string, detail: string, picked: boolean): Failure {
  switch (code) {
    case "newerBackup":
      return newerFailure(detail);
    case "noRoom":
      return { kind: "noRoom" };
    case "notFound":
      return picked ? { kind: "bad" } : { kind: "gone" };
    case "damagedBackup":
      return picked ? { kind: "bad" } : { kind: "damaged" };
    default:
      return { kind: "bad" };
  }
}

/** Serves #/privacy/restore. */
export default function Restore() {
  return <RestoreFlow mode="privacy" />;
}

/** Every step of bringing a backup back, shared by Privacy and setup. */
export function RestoreFlow({ mode = "privacy", onExit, onContinue }: RestoreFlowProps) {
  const engine = useEngine();
  const platform = usePlatform();
  const refresh = useRefresh();

  const [step, setStep] = useState<Step>("choose");
  const [path, setPath] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [show, setShow] = useState(false);
  const [error, setError] = useState("");
  const [attempts, setAttempts] = useState(0);
  const [check, setCheck] = useState<RestoreCheck | null>(null);
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [stopped, setStopped] = useState(false);
  const [restored, setRestored] = useState<RestoredView | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [advanced, setAdvanced] = useState(false);

  const titleRef = useRef<HTMLHeadingElement>(null);
  const fieldRef = useRef<HTMLInputElement>(null);
  const resultRef = useRef<HTMLDivElement>(null);
  const firstStep = useRef(true);
  const working = useRef<"check" | "restore" | null>(null);
  const advancedId = useId();

  const privacy = useQuery({
    queryKey: ["privacy"],
    queryFn: () => engine.getPrivacy(),
    staleTime: 0,
  });
  const setup = useQuery({
    queryKey: ["setup"],
    queryFn: () => engine.getSetup(),
    staleTime: 0,
  });

  useEngineEvent("backup-progress", (next) => {
    const wanted =
      working.current === "check" ? "checking" : working.current === "restore" ? "restoring" : null;
    if (next.phase === wanted) setProgress(next);
  });

  // A restore that finished while the window was away opens on the card that says so.
  useEffect(() => {
    let live = true;
    engine.getRestored().then(
      (view) => {
        if (!live || !view) return;
        setRestored(view);
        setStep("restored");
        setAnnouncement(t("restore.announce.restored"));
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [engine]);

  useEffect(() => {
    if (firstStep.current) {
      firstStep.current = false;
      return;
    }
    if (step === "passphrase") {
      fieldRef.current?.focus({ preventScroll: true });
      return;
    }
    titleRef.current?.focus({ preventScroll: true });
  }, [step]);

  useEffect(() => {
    if (error) fieldRef.current?.focus({ preventScroll: true });
  }, [error]);

  useEffect(() => {
    if (failure) resultRef.current?.querySelector("button")?.focus({ preventScroll: true });
  }, [failure]);

  useEffect(() => {
    if (step === "restoring") document.body.setAttribute("aria-busy", "true");
    else document.body.removeAttribute("aria-busy");
    return () => document.body.removeAttribute("aria-busy");
  }, [step]);

  const fullWindow = mode === "privacy" && (step === "restoring" || step === "restored");

  useEffect(() => {
    if (!fullWindow) return;
    const root = document.getElementById("root");
    root?.setAttribute("inert", "");
    return () => root?.removeAttribute("inert");
  }, [fullWindow]);

  const eta = useProgressEta(progress);
  const final =
    step === "restoring" &&
    progress !== null &&
    progress.total > 0 &&
    progress.done >= progress.total;

  const leave = () => {
    void engine.discardRestore().catch(() => undefined);
    if (mode === "setup") {
      if (onExit) onExit();
      else goTo("/setup");
      return;
    }
    goTo("/privacy");
  };

  const chooseFile = async () => {
    setFailure(null);
    setError("");
    setAttempts(0);
    setCheck(null);
    setProgress(null);
    setStopped(false);
    let chosen: string | null;
    try {
      chosen = await platform.pickBackup();
    } catch {
      setFailure({ kind: "bad" });
      return;
    }
    if (chosen === null) return;
    try {
      await engine.probeBackup(chosen);
    } catch (raw) {
      const failureFromEngine = toEngineError(raw);
      setPath(null);
      setPassphrase("");
      setFailure(failureFrom(failureFromEngine.code, failureFromEngine.detail, true));
      setStep("choose");
      return;
    }
    setPath(chosen);
    setPassphrase("");
    setStep("passphrase");
  };

  const checkBackup = () => {
    if (path === null) return;
    setFailure(null);
    setStopped(false);
    setProgress(null);
    setStep("checking");
    setAnnouncement(t("restore.announce.checking"));
    working.current = "check";
    engine.checkBackup(path, passphrase).then(
      (result) => {
        working.current = null;
        setProgress(null);
        setError("");
        setAttempts(0);
        setCheck(result);
        setStep(result.here ? "confirm" : "ready");
      },
      (raw) => {
        working.current = null;
        setProgress(null);
        const failureFromEngine = toEngineError(raw);
        if (failureFromEngine.code === "wrongPassphrase") {
          const next = attempts + 1;
          setAttempts(next);
          setError(t(next >= 3 ? "restore.wrong.many" : "restore.wrong"));
          setStep("passphrase");
          return;
        }
        if (failureFromEngine.code === "cancelled") {
          setAnnouncement("");
          setStep("passphrase");
          return;
        }
        const failed = failureFrom(failureFromEngine.code, failureFromEngine.detail, false);
        setFailure(failed);
        setStep(failed.kind === "bad" || failed.kind === "newer" ? "passphrase" : "checking");
      },
    );
  };

  const startRestore = () => {
    if (path === null || check === null) return;
    setFailure(null);
    setStopped(false);
    setProgress(null);
    setStep("restoring");
    setAnnouncement(t("restore.announce.restoring"));
    working.current = "restore";
    engine.restoreBackup(path, passphrase, check.here !== null).then(
      async () => {
        working.current = null;
        setProgress(null);
        let view: RestoredView | null = null;
        try {
          view = await engine.getRestored();
        } catch {
          view = null;
        }
        setRestored(
          view ?? {
            madeAt: check.summary.madeAt,
            memories: check.summary.memories,
            pages: check.summary.pages,
            originals: check.summary.originals,
            onlineUsed: check.summary.onlineUsed,
            awayUsed: check.summary.awayUsed,
            putAsideAt: check.here ? new Date().toISOString() : null,
          },
        );
        refresh("privacy", "backups", "page", "pages", "items", "memories", "needsYou", "fading");
        setStep("restored");
        setAnnouncement(t("restore.announce.restored"));
      },
      (raw) => {
        working.current = null;
        setProgress(null);
        const failureFromEngine = toEngineError(raw);
        if (failureFromEngine.code === "cancelled") {
          setStopped(true);
          setAnnouncement("");
          setStep(check.here ? "confirm" : "ready");
          return;
        }
        setFailure(failureFrom(failureFromEngine.code, failureFromEngine.detail, false));
        setStep(check.here ? "confirm" : "ready");
      },
    );
  };

  const stop = () => {
    void engine.cancelBackup().catch(() => undefined);
  };

  const openToday = () => {
    void engine.finishRestore().catch(() => undefined);
    refresh("privacy", "backups", "page", "pages", "items", "memories", "needsYou", "fading");
    if (mode === "setup") {
      if (restored && onContinue) onContinue(restored);
      else goTo("/setup");
      return;
    }
    goTo("/today");
  };

  const title =
    step === "checking"
      ? t("restore.checking.title")
      : step === "ready"
        ? t("restore.ready.title")
        : step === "confirm"
          ? t("restore.confirm.title")
          : step === "restoring"
            ? t("restore.restoring.title")
            : step === "restored"
              ? t("restore.restored.title")
              : t("restore.title");

  const aside =
    mode === "privacy" && ["choose", "passphrase", "checking"].includes(step) ? (
      <aside className={layout.aside}>
        <h2 className={layout.label}>{t("restore.aside.what")}</h2>
        <ul className={layout.rules}>
          <li>{t("restore.aside.checks")}</li>
          <li>{t("restore.aside.brings")}</li>
          <li>{t("restore.aside.puts")}</li>
        </ul>
        <h2 className={layout.label}>{t("restore.aside.after")}</h2>
        <ul className={`${layout.rules} ${layout.out}`}>
          <li>
            <strong>{t("restore.aside.online.title")}</strong> {t("restore.aside.online.body")}
          </li>
          <li>
            <strong>{t("restore.aside.away.title")}</strong> {t("restore.aside.away.body")}
          </li>
        </ul>
      </aside>
    ) : null;

  const live = (
    <p className="sr-only" role="status" aria-live="polite">
      {announcement}
    </p>
  );

  const heading = (className: string | undefined) => (
    <h1 ref={titleRef} tabIndex={-1} className={className}>
      {title}
    </h1>
  );

  const failureBox = failure ? (
    <FailureBox
      failure={failure}
      check={check}
      free={setup.data ? formatBytes(setup.data.freeDiskBytes) : null}
      boxRef={resultRef}
      onChooseAnother={() => void chooseFile()}
      onRetry={failure.kind === "noRoom" ? checkBackup : checkBackup}
    />
  ) : null;

  const chooseStep = (
    <>
      <p className={layout.lead}>{t(mode === "setup" ? "restore.lead.setup" : "restore.lead")}</p>
      {failure ? (
        failureBox
      ) : (
        <ActionBar>
          <Button variant="primary" onClick={() => void chooseFile()}>
            {t("restore.choose")}
          </Button>
          <Button variant="quiet" onClick={leave}>
            {t(mode === "setup" ? "restore.back" : "dialog.cancel")}
          </Button>
        </ActionBar>
      )}
    </>
  );

  const passphraseStep = (
    <>
      <div className={styles.fileRow}>
        <span className={styles.fileText}>
          <strong>{path ? fileNameOf(path) : ""}</strong>
        </span>
        <Button variant="quiet" onClick={() => void chooseFile()}>
          {t("restore.file.chooseAnother")}
        </Button>
      </div>
      <div className={layout.fields}>
        <TextInput
          label={t("restore.field.label")}
          value={passphrase}
          type={show ? "text" : "password"}
          autoComplete="current-password"
          spellCheck={false}
          inputRef={fieldRef}
          error={error || undefined}
          onChange={(next) => {
            setPassphrase(next);
            if (error) setError("");
          }}
        />
        <Checkbox label={t("restore.show")} checked={show} onChange={() => setShow(!show)} />
      </div>
      {failure ? (
        failureBox
      ) : (
        <ActionBar>
          <Button variant="primary" onClick={checkBackup}>
            {t("restore.open")}
          </Button>
          <Button variant="quiet" onClick={leave}>
            {t(mode === "setup" ? "restore.back" : "dialog.cancel")}
          </Button>
        </ActionBar>
      )}
    </>
  );

  const checkingStep = failure ? (
    failureBox
  ) : (
    <>
      <Progress
        label={t("restore.checking.label")}
        value={progress?.done ?? 0}
        max={progress?.total ?? 0}
        detail={progress ? progressDetail(progress, eta) : undefined}
      />
      <p className={layout.lead}>{t("restore.checking.body")}</p>
      <ActionBar>
        <Button variant="secondary" onClick={stop}>
          {t("restore.stop")}
        </Button>
      </ActionBar>
    </>
  );

  const summary = check?.summary ?? null;

  const readyStep = (
    <>
      {stopped ? (
        <div className={layout.result} role="status">
          <p>{t("restore.stopped")}</p>
        </div>
      ) : null}
      {failure ? failureBox : null}
      {summary ? (
        <dl className={layout.receipt}>
          <div>
            <dt>{t("restore.ready.made")}</dt>
            <dd>{formatDayMonth(summary.madeAt)}</dd>
          </div>
          <div>
            <dt>{t("restore.ready.holds")}</dt>
            <dd>
              {t("restore.ready.holds.value", {
                memories: summary.memories,
                pages: summary.pages,
                originals: summary.originals,
              })}
            </dd>
          </div>
          <div>
            <dt>{t("restore.ready.size")}</dt>
            <dd>{formatBytes(summary.bytes)}</dd>
          </div>
          <div>
            <dt>{t("restore.ready.checked")}</dt>
            <dd className={layout.ticked}>{t("restore.ready.checked.value")}</dd>
          </div>
        </dl>
      ) : null}
      <ActionBar>
        <Button variant="primary" onClick={startRestore}>
          {t("restore.restore")}
        </Button>
        <Button variant="quiet" onClick={leave}>
          {t(mode === "setup" ? "restore.back" : "dialog.cancel")}
        </Button>
      </ActionBar>
    </>
  );

  const here = check?.here ?? null;
  const confirmStep = (
    <>
      {stopped ? (
        <div className={layout.result} role="status">
          <p>{t("restore.stopped")}</p>
        </div>
      ) : null}
      <p className={layout.lead}>{tc("restore.confirm.body")}</p>
      {summary && here ? (
        <div className={styles.compare}>
          <section className={styles.side} data-incoming>
            <div className={styles.sideHead}>
              <h2 className={styles.sideName}>{t("restore.confirm.in")}</h2>
              <span className={styles.sideFate}>{t("restore.confirm.in.note")}</span>
            </div>
            <dl>
              <div>
                <dt>{t("restore.confirm.made")}</dt>
                <dd>{formatDayMonth(summary.madeAt)}</dd>
              </div>
              <div>
                <dt>{t("restore.confirm.memories")}</dt>
                <dd>{summary.memories}</dd>
              </div>
              <div>
                <dt>{t("restore.confirm.pages")}</dt>
                <dd>{summary.pages}</dd>
              </div>
              <div>
                <dt>{t("restore.confirm.originals")}</dt>
                <dd>{summary.originals}</dd>
              </div>
            </dl>
          </section>
          <section className={styles.side} data-aside>
            <div className={styles.sideHead}>
              <h2 className={styles.sideName}>{t("restore.confirm.here")}</h2>
              <span className={styles.sideFate}>{t("restore.confirm.here.note")}</span>
            </div>
            <dl>
              {here.lastKept ? (
                <div>
                  <dt>{t("restore.confirm.lastKept")}</dt>
                  <dd>{formatDayMonth(here.lastKept)}</dd>
                </div>
              ) : null}
              <div>
                <dt>{t("restore.confirm.memories")}</dt>
                <dd>{here.memories}</dd>
              </div>
              <div>
                <dt>{t("restore.confirm.pages")}</dt>
                <dd>{here.pages}</dd>
              </div>
              <div>
                <dt>{t("restore.confirm.originals")}</dt>
                <dd>{here.originals}</dd>
              </div>
              {here.keptAfterBackup > 0 ? (
                <div>
                  <dt>{t("restore.confirm.keptAfter")}</dt>
                  <dd>{here.keptAfterBackup}</dd>
                </div>
              ) : null}
            </dl>
          </section>
        </div>
      ) : null}
      {check?.putAside ? (
        <p className={layout.small}>
          {t("restore.confirm.already", { day: formatDayMonth(check.putAside) })}
        </p>
      ) : null}
      {failure ? failureBox : null}
      <ActionBar>
        <Button variant="secondary" onClick={leave}>
          {t("restore.confirm.keep")}
        </Button>
        <Button variant="primary" onClick={startRestore}>
          {t("restore.confirm.replace")}
        </Button>
      </ActionBar>
    </>
  );

  const restoringStep = (
    <>
      <Progress
        label={t(final ? "restore.restoring.final" : "restore.restoring.label")}
        value={progress?.done ?? 0}
        max={progress?.total ?? 0}
        detail={progress ? progressDetail(progress, final ? null : eta) : undefined}
      />
      <p className={layout.lead}>{t("restore.restoring.body")}</p>
      <ActionBar line={final ? t("restore.restoring.finalNote") : undefined}>
        <Button variant="secondary" disabled={final} onClick={stop}>
          {t("restore.stop")}
        </Button>
      </ActionBar>
    </>
  );

  const view = restored;
  const mindSize = privacy.data?.mindBytes ?? setup.data?.totalBytes ?? 0;
  const restoredStep = view ? (
    <>
      <p className={layout.lead}>
        {t("restore.restored.lead", {
          memories: view.memories,
          pages: view.pages,
          originals: view.originals,
          day: formatDayMonth(view.madeAt),
        })}
      </p>
      <dl className={styles.dl}>
        <div>
          <dt>{t("restore.restored.mind")}</dt>
          <dd>
            {t(
              mode === "setup" ? "restore.restored.mind.missing" : "restore.restored.mind.here",
              mode === "setup" ? { size: formatBytes(mindSize) } : undefined,
            )}
          </dd>
        </div>
        {view.onlineUsed ? (
          <div>
            <dt>{t("restore.restored.online")}</dt>
            <dd>
              <span>
                {tc(
                  mode === "setup"
                    ? "restore.restored.online.later"
                    : "restore.restored.online.body",
                )}
              </span>
              {mode === "privacy" ? (
                <a className="hit" href="#/settings/online">
                  {t("restore.restored.online.link")}
                </a>
              ) : null}
            </dd>
          </div>
        ) : null}
        {view.awayUsed ? (
          <div>
            <dt>{t("restore.restored.away")}</dt>
            <dd>
              <span>{t("restore.restored.away.body")}</span>
              {mode === "privacy" ? (
                <a className="hit" href="#/settings/away">
                  {t("restore.restored.away.link")}
                </a>
              ) : null}
            </dd>
          </div>
        ) : null}
        {view.putAsideAt ? (
          <div>
            <dt>{t("restore.restored.before")}</dt>
            <dd>
              <span>
                {tc("restore.restored.before.body", { day: formatDayMonth(view.putAsideAt) })}
              </span>
            </dd>
          </div>
        ) : null}
      </dl>
      <ActionBar>
        <Button variant="primary" onClick={openToday}>
          {t(mode === "setup" ? "restore.restored.continue" : "restore.restored.open")}
        </Button>
      </ActionBar>
    </>
  ) : null;

  const main = (
    <section className={layout.main}>
      {live}
      {heading(layout.stepTitle)}
      {step === "choose" ? chooseStep : null}
      {step === "passphrase" ? passphraseStep : null}
      {step === "checking" ? checkingStep : null}
      {step === "ready" ? readyStep : null}
      {step === "confirm" ? confirmStep : null}
      {step === "restoring" ? restoringStep : null}
      {step === "restored" ? restoredStep : null}
      {["choose", "passphrase", "checking", "ready", "confirm"].includes(step) ? (
        <Advanced
          id={advancedId}
          open={advanced}
          onToggle={() => setAdvanced(!advanced)}
          path={path}
          check={check}
          dir={privacy.data?.dataDir ?? null}
        />
      ) : null}
    </section>
  );

  if (fullWindow) {
    return createPortal(
      <div className={styles.window}>
        <section className={styles.card} aria-busy={step === "restoring"}>
          <ActionBarHost>
            {live}
            {heading(`${layout.stepTitle} ${styles.cardTitle}`)}
            {step === "restoring" ? restoringStep : null}
            {step === "restored" ? restoredStep : null}
          </ActionBarHost>
        </section>
      </div>,
      document.body,
    );
  }

  if (mode === "setup") {
    return <div className={styles.inline}>{main}</div>;
  }

  return (
    <ScreenSheet>
      <BackLink to="/privacy" label={t("screen.privacy.title")} />
      <div className={layout.columns}>
        {main}
        {aside}
      </div>
    </ScreenSheet>
  );
}

function FailureBox({
  failure,
  check,
  free,
  boxRef,
  onChooseAnother,
  onRetry,
}: {
  failure: Failure;
  check: RestoreCheck | null;
  free: string | null;
  boxRef: RefObject<HTMLDivElement | null>;
  onChooseAnother: () => void;
  onRetry: () => void;
}) {
  const title =
    failure.kind === "newer"
      ? t("restore.newer.title")
      : failure.kind === "damaged"
        ? t("restore.damaged.title")
        : failure.kind === "gone"
          ? t("restore.gone.title")
          : failure.kind === "noRoom"
            ? t("restore.noRoom.title")
            : t("restore.bad.title");
  const body =
    failure.kind === "newer"
      ? failure.theirs
        ? t("restore.newer.body", { theirs: failure.theirs, ours: APP_VERSION })
        : t("restore.newer.body.generic")
      : failure.kind === "damaged"
        ? t("restore.damaged.body")
        : failure.kind === "gone"
          ? t("restore.gone.body")
          : failure.kind === "noRoom"
            ? check && free
              ? t("restore.noRoom.body", {
                  needed: formatBytes(check.summary.bytes),
                  free,
                })
              : t("restore.noRoom.body.generic")
            : t("restore.bad.body");
  const action =
    failure.kind === "gone" ? (
      <Button variant="secondary" onClick={onRetry}>
        {t("restore.tryAgain")}
      </Button>
    ) : failure.kind === "noRoom" ? (
      <Button variant="secondary" onClick={onRetry}>
        {t("restore.checkAgain")}
      </Button>
    ) : (
      <Button variant="secondary" onClick={onChooseAnother}>
        {t("restore.chooseFile")}
      </Button>
    );
  return (
    <div ref={boxRef} className={layout.result} data-kind="bad" role="alert">
      <p className={layout.resultTitle}>{title}</p>
      <p>{body}</p>
      <div className={layout.buttons}>{action}</div>
    </div>
  );
}

function Advanced({
  id,
  open,
  onToggle,
  path,
  check,
  dir,
}: {
  id: string;
  open: boolean;
  onToggle: () => void;
  path: string | null;
  check: RestoreCheck | null;
  dir: string | null;
}) {
  const summary = check?.summary ?? null;
  const date = summary ? `${formatDayMonth(summary.madeAt)}, ${formatTime(summary.madeAt)}` : "";
  const rows: Array<[string, string]> = [
    [
      t("advanced.restore.file"),
      path
        ? t("advanced.restore.file.value", { file: fileNameOf(path) })
        : t("advanced.restore.file.pending"),
    ],
    [
      t("advanced.restore.format"),
      summary
        ? t("advanced.restore.format.value", {
            format: 1,
            schema: summary.storeVersion,
            version: summary.appVersion,
            date,
          })
        : t("advanced.restore.format.pending"),
    ],
    [t("advanced.backup.locking"), t("advanced.backup.locking.value")],
    [
      t("advanced.restore.contents"),
      summary
        ? t("advanced.restore.contents.value", {
            size: formatBytes(summary.bytes),
            memories: summary.memories,
            pages: summary.pages,
            originals: summary.originals,
          })
        : t("advanced.restore.contents.pending"),
    ],
    [
      t("advanced.restore.checked"),
      summary ? t("advanced.restore.checked.value") : t("advanced.restore.checked.pending"),
    ],
    [t("advanced.backup.leftOut"), t("advanced.backup.leftOut.value")],
  ];
  if (dir !== null) {
    rows.push([t("advanced.backup.putAside"), t("advanced.backup.putAside.value", { dir })]);
  }
  return (
    <div className={layout.more}>
      <button
        type="button"
        className={`${layout.moreButton} hit`}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={onToggle}
      >
        <Chevron />
        {t("advanced.title")}
      </button>
      {open ? (
        <dl id={id} className={layout.advanced}>
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
