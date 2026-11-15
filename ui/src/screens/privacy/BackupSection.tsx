import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { Progress } from "../../components/Progress/Progress";
import { formatBytes, formatDayMonth, formatEta } from "../../copy/format";
import { t } from "../../copy/t";
import type { BackupProgress, PutAsideView } from "../../engine/bindings";
import { useEngine, useEngineEvent, useRefresh } from "../../engine/context";
import layout from "../backup/Backup.module.css";
import { progressDetail, useProgressEta } from "../backup/progress";
import { clearBackup, useBackupAttempt } from "../backup/run";
import restoreStyles from "../restore/Restore.module.css";
import styles from "./Privacy.module.css";

/** The Backup block on Privacy: one file, its last run, and the way in to making another. */
export function BackupSection() {
  const engine = useEngine();
  const refresh = useRefresh();
  const attempt = useBackupAttempt();
  const [live, setLive] = useState<BackupProgress | null>(null);
  const [finished, setFinished] = useState(false);
  const [bringingBack, setBringingBack] = useState<string | null>(null);
  const [bringBackCopy, setBringBackCopy] = useState<PutAsideView | null>(null);
  const [deleteCopy, setDeleteCopy] = useState<PutAsideView | null>(null);
  const [deleting, setDeleting] = useState(false);
  const lastQuery = useRef<BackupProgress | null>(null);

  const backups = useQuery({
    queryKey: ["backups"],
    queryFn: () => engine.getBackups(),
    staleTime: 0,
    refetchInterval: (query) => (query.state.data?.running ? 1000 : false),
  });
  const restored = useQuery({
    queryKey: ["restored"],
    queryFn: () => engine.getRestored(),
    staleTime: 0,
  });

  // A restore is another screen's work, not a backup, even though it travels on this event.
  useEngineEvent("backup-progress", (progress) => {
    if (progress.phase !== "restoring") setLive(progress);
  });

  useEffect(() => {
    if (!attempt?.outcome) return;
    setFinished(true);
    setLive(null);
    refresh("backups", "privacy");
    clearBackup();
  }, [attempt, refresh]);

  // Without a run this window started (a reopened window), the poll is what says it has ended.
  useEffect(() => {
    const running = backups.data?.running ?? null;
    if (lastQuery.current && !running) {
      setLive(null);
      refresh("privacy");
    }
    lastQuery.current = running;
  }, [backups.data, refresh]);

  const queried = backups.data?.running ?? null;
  const running = finished || queried?.phase === "restoring" ? null : (live ?? queried ?? null);
  const eta = useProgressEta(running);

  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running) refresh("backups", "privacy");
    wasRunning.current = Boolean(running);
  }, [running, refresh]);

  const notices = Boolean(backups.data?.interruptedBackup || backups.data?.interruptedRestore);
  const dismissNotices = () => {
    if (!notices) return;
    void engine.dismissBackupNotices().then(
      () => refresh("backups"),
      () => undefined,
    );
  };

  const make = () => {
    dismissNotices();
    goTo("/privacy/backup");
  };

  const restore = () => {
    dismissNotices();
    goTo("/privacy/restore");
  };

  const detail = running
    ? eta === null
      ? t("progress.of", {
          done: formatBytes(running.done),
          total: formatBytes(running.total),
        })
      : t("privacy.backup.detail", {
          done: formatBytes(running.done),
          total: formatBytes(running.total),
          eta: formatEta(eta),
        })
    : undefined;

  const last = backups.data?.last ?? null;
  const interruptedBackup = Boolean(backups.data?.interruptedBackup) && !running;
  const interruptedRestore = Boolean(backups.data?.interruptedRestore);
  const restoreNotice = interruptedRestore
    ? t(restored.data ? "privacy.backup.restore.finished" : "privacy.backup.restore.kept")
    : null;
  const putAside = backups.data?.putAside ?? [];

  const confirmDelete = () => {
    if (!deleteCopy) return;
    setDeleting(true);
    engine.deletePutAside(deleteCopy.id).then(
      () => {
        setDeleting(false);
        setDeleteCopy(null);
        refresh("backups", "privacy");
      },
      () => {
        setDeleting(false);
        setDeleteCopy(null);
        refresh("backups", "privacy");
      },
    );
  };

  return (
    <section aria-labelledby="backup" className={styles.block}>
      <h2 id="backup" className={styles.blockTitle}>
        {t("privacy.backup.title")}
      </h2>
      <p>{t("privacy.backup.body")}</p>
      {running ? (
        <>
          <Progress
            label={t("privacy.backup.running")}
            value={running.done}
            max={running.total}
            detail={detail}
          />
          <div>
            <Button variant="secondary" onClick={() => goTo("/privacy/backup")}>
              {t("privacy.backup.see")}
            </Button>
          </div>
        </>
      ) : (
        <>
          {last ? (
            <p className={styles.lastBackup}>
              {t("privacy.backup.last", {
                day: formatDayMonth(last.at),
                size: formatBytes(last.bytes),
              })}
            </p>
          ) : null}
          <div className={styles.backupButtons}>
            <Button variant="secondary" onClick={make}>
              {t("privacy.backup.make")}
            </Button>
            <Button variant="quiet" onClick={restore}>
              {t("privacy.backup.restore")}
            </Button>
          </div>
        </>
      )}
      {interruptedBackup ? (
        <div className={styles.backupNotice} role="status">
          <p>{t("privacy.backup.interrupted")}</p>
          <Button variant="quiet" onClick={make}>
            {t("privacy.backup.interrupted.action")}
          </Button>
        </div>
      ) : null}
      {restoreNotice ? (
        <div className={styles.backupNotice} role="status">
          <p>{restoreNotice}</p>
        </div>
      ) : null}
      {putAside.map((copy) => (
        <div className={styles.putAsideRow} key={copy.id}>
          <span className={styles.putAsideText}>
            <strong>{t("privacy.backup.putAside.label")}</strong>
            <span className={styles.mono}>
              {t("privacy.backup.putAside.detail", {
                day: formatDayMonth(copy.at),
                memories: copy.memories,
                size: formatBytes(copy.bytes),
              })}
            </span>
          </span>
          <span className={styles.putAsideButtons}>
            <Button variant="quiet" onClick={() => setBringBackCopy(copy)}>
              {t("privacy.backup.putAside.bringBack")}
            </Button>
            <Button variant="forget" onClick={() => setDeleteCopy(copy)}>
              {t("privacy.backup.putAside.delete")}
            </Button>
          </span>
        </div>
      ))}
      {bringBackCopy ? (
        <BringBackDialog
          onClose={() => setBringBackCopy(null)}
          onConfirm={() => {
            setBringingBack(bringBackCopy.id);
            setBringBackCopy(null);
          }}
        />
      ) : null}
      {deleteCopy ? (
        <DeletePutAsideDialog
          copy={deleteCopy}
          busy={deleting}
          onClose={() => setDeleteCopy(null)}
          onConfirm={confirmDelete}
        />
      ) : null}
      {bringingBack ? (
        <BringBackCard id={bringingBack} onDone={() => setBringingBack(null)} />
      ) : null}
    </section>
  );
}

function BringBackDialog({ onClose, onConfirm }: { onClose: () => void; onConfirm: () => void }) {
  const notNow = useRef<HTMLElement | null>(null);
  return (
    <Dialog title={t("privacy.backup.bringBack.title")} onClose={onClose} initialFocus={notNow}>
      <p className={styles.dialogBody}>{t("privacy.backup.bringBack.body")}</p>
      <div
        className={styles.dialogActions}
        ref={(el) => {
          notNow.current = el?.querySelector("button") ?? null;
        }}
      >
        <Button variant="secondary" onClick={onClose}>
          {t("privacy.backup.bringBack.notNow")}
        </Button>
        <Button variant="primary" onClick={onConfirm}>
          {t("privacy.backup.bringBack.confirm")}
        </Button>
      </div>
    </Dialog>
  );
}

function DeletePutAsideDialog({
  copy,
  busy,
  onClose,
  onConfirm,
}: {
  copy: PutAsideView;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const keep = useRef<HTMLElement | null>(null);
  return (
    <Dialog
      title={t("privacy.backup.delete.title")}
      onClose={onClose}
      initialFocus={keep}
      locked={busy}
    >
      <p className={styles.dialogBody}>
        {t("privacy.backup.delete.body", {
          memories: copy.memories,
          pages: copy.pages,
          originals: copy.originals,
          day: formatDayMonth(copy.at),
        })}
      </p>
      <div
        className={styles.dialogActions}
        ref={(el) => {
          keep.current = el?.querySelector("button") ?? null;
        }}
      >
        <Button variant="secondary" disabled={busy} onClick={onClose}>
          {t("privacy.backup.delete.keep")}
        </Button>
        <Button variant="forget" confirming disabled={busy} onClick={onConfirm}>
          {t("privacy.backup.delete.confirm")}
        </Button>
      </div>
    </Dialog>
  );
}

/** The Restoring card, with the words for bringing a put-aside copy back. */
function BringBackCard({ id, onDone }: { id: string; onDone: () => void }) {
  const engine = useEngine();
  const refresh = useRefresh();
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [stopping, setStopping] = useState(false);
  const title = useRef<HTMLHeadingElement>(null);
  const done = useRef(onDone);
  done.current = onDone;
  const eta = useProgressEta(progress);
  const final = progress !== null && progress.total > 0 && progress.done >= progress.total;

  useEngineEvent("backup-progress", (next) => {
    if (next.phase === "restoring") setProgress(next);
  });

  useEffect(() => {
    title.current?.focus({ preventScroll: true });
    document.body.setAttribute("aria-busy", "true");
    const root = document.getElementById("root");
    root?.setAttribute("inert", "");
    return () => {
      document.body.removeAttribute("aria-busy");
      root?.removeAttribute("inert");
    };
  }, []);

  useEffect(() => {
    let live = true;
    engine.restorePutAside(id).then(
      () => {
        if (!live) return;
        refresh("backups", "privacy", "page", "pages", "items", "memories", "needsYou", "fading");
        done.current();
      },
      () => {
        if (!live) return;
        refresh("backups", "privacy");
        done.current();
      },
    );
    return () => {
      live = false;
    };
  }, [engine, id, refresh]);

  const stop = () => {
    setStopping(true);
    void engine.cancelBackup().catch(() => setStopping(false));
  };

  return createPortal(
    <div className={restoreStyles.window}>
      <section className={restoreStyles.card} aria-busy="true">
        <h1 ref={title} tabIndex={-1} className={`${layout.stepTitle} ${restoreStyles.cardTitle}`}>
          {t("privacy.backup.bringBack.restoring")}
        </h1>
        <Progress
          label={t(final ? "restore.restoring.final" : "restore.restoring.label")}
          value={progress?.done ?? 0}
          max={progress?.total ?? 0}
          detail={progress ? progressDetail(progress, final ? null : eta) : undefined}
        />
        <p className={layout.lead}>{t("restore.restoring.body")}</p>
        <div className={layout.footer}>
          <Button variant="secondary" disabled={final || stopping} onClick={stop}>
            {t("restore.stop")}
          </Button>
          {final ? <span className={layout.small}>{t("restore.restoring.finalNote")}</span> : null}
        </div>
      </section>
    </div>,
    document.body,
  );
}
