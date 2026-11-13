import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Progress } from "../../components/Progress/Progress";
import { useToast } from "../../components/Toast/Toast";
import { Toggle } from "../../components/Toggle/Toggle";
import { formatBytes, formatDayMonth, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { ExportProgress, ExportReport, LedgerEntry } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { patch } from "../settings/patch";
import { DeleteEverythingDialog } from "./DeleteEverythingDialog";
import styles from "./Privacy.module.css";

/** Seconds are counted up to a minute; after an hour the clock time is clearer than a count. */
function checkedText(checkedAt: string, now: number): string {
  const seconds = Math.round((now - new Date(checkedAt).getTime()) / 1000);
  if (Number.isNaN(seconds) || seconds < 0 || seconds >= 3600) {
    return t("privacy.watching.at", { time: formatTime(checkedAt) });
  }
  const ago =
    seconds < 60
      ? t("privacy.ago.seconds", { count: Math.max(1, seconds) })
      : t("privacy.ago.minutes", { count: Math.round(seconds / 60) });
  return t("privacy.watching", { ago });
}

function ledgerLabel(entry: LedgerEntry): string {
  const id = {
    setup: "privacy.ledger.setup",
    appUpdate: "privacy.ledger.appUpdate",
    mindUpdate: "privacy.ledger.mindUpdate",
    updateCheck: "privacy.ledger.updateCheck",
  } as const;
  return t(id[entry.kind], { label: entry.label });
}

type Exporting =
  | { state: "idle" }
  | { state: "running"; progress: ExportProgress | null }
  | { state: "done"; report: ExportReport }
  | { state: "failed" };

/** Serves #/privacy: proof, not promise. */
export default function Privacy() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const refresh = useRefresh();
  const query = useQuery({
    queryKey: ["privacy"],
    queryFn: () => engine.getPrivacy(),
    refetchInterval: 15_000,
  });
  const [now, setNow] = useState(() => Date.now());
  const [exporting, setExporting] = useState<Exporting>({ state: "idle" });
  const [lock, setLock] = useState<boolean | null>(null);
  const [deleting, setDeleting] = useState(false);
  const exportBox = useRef<HTMLDivElement>(null);

  // Only the words "checked N seconds ago" change with time; they are not a live region.
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 5000);
    return () => clearInterval(timer);
  }, []);

  const running = exporting.state === "running";
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running) exportBox.current?.querySelector("button")?.focus();
    wasRunning.current = running;
  }, [running]);

  const exportEverything = async () => {
    let dir: string | null;
    try {
      dir = await platform.pickFolder();
    } catch {
      setExporting({ state: "failed" });
      return;
    }
    if (dir === null) return;
    setExporting({ state: "running", progress: null });
    const off = engine.on("export-progress", (progress) =>
      setExporting({ state: "running", progress }),
    );
    try {
      setExporting({ state: "done", report: await engine.exportTo(dir) });
    } catch {
      setExporting({ state: "failed" });
    } finally {
      off();
    }
  };

  const changeLock = async (next: boolean) => {
    setLock(next);
    try {
      await engine.updateSettings(patch({ lock: next }));
      refresh("privacy", "settings");
    } catch {
      setLock(null);
      toast.show({ message: t("privacy.lock.failed") });
    }
  };

  if (query.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("privacy.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  const view = query.data;
  if (!view) {
    return (
      <p className={styles.message} role="status">
        {t("privacy.loading")}
      </p>
    );
  }

  const measured = view.sentMeasured;
  const total = view.keptBytes + view.mindBytes;
  const lockOn = lock ?? view.lock;

  return (
    <div className={styles.frame}>
      <div className={styles.card}>
        <div className={styles.proof}>
          <h1 className={`t-section ${styles.eyebrow}`}>{t("privacy.title")}</h1>
          <p className={styles.sentLabel}>{t("privacy.sent.label")}</p>
          <p className={`t-display ${styles.nothing}`}>
            {!measured
              ? t("privacy.sent.unmeasured")
              : view.sentToInternetBytes > 0
                ? t("privacy.sent.amount", { size: formatBytes(view.sentToInternetBytes) })
                : t("privacy.sent.nothing")}
          </p>
          {measured ? (
            <p className={styles.watching}>
              <span className={styles.dot} aria-hidden="true" />
              {view.checkedAt ? checkedText(view.checkedAt, now) : t("privacy.watching.now")}
            </p>
          ) : (
            <p className={styles.detail}>{t("privacy.sent.unmeasured.detail")}</p>
          )}

          <section aria-labelledby="ledger" className={styles.ledger}>
            <h2 id="ledger" className="t-section">
              {t("privacy.ledger.heading")}
            </h2>
            {view.ledger.length === 0 ? (
              <p className={styles.detail}>{t("privacy.ledger.empty")}</p>
            ) : (
              <ul>
                {view.ledger.map((entry) => (
                  <li key={`${entry.kind}-${entry.label}-${entry.at}`}>
                    <span>{ledgerLabel(entry)}</span>
                    <span className={styles.mono}>
                      {t("privacy.ledger.entry", {
                        size: formatBytes(entry.bytes),
                        day: formatDayMonth(entry.at),
                      })}
                    </span>
                  </li>
                ))}
                <li className={styles.quiet}>
                  <span>{t("privacy.updates.label")}</span>
                  <span className={styles.mono}>
                    {t(view.checkUpdates ? "settings.updates.on" : "settings.updates.off")}
                  </span>
                </li>
              </ul>
            )}
            <a className={styles.link} href="#/settings">
              {t("privacy.updates.change")}
            </a>
          </section>

          <p className={styles.promise}>{t("privacy.promise")}</p>
        </div>

        <div className={styles.controls}>
          <section aria-labelledby="here">
            <h2 id="here" className="t-section">
              {t("privacy.here.heading")}
            </h2>
            <p className={styles.sizes}>
              <strong>{t("privacy.here.kept", { size: formatBytes(view.keptBytes) })}</strong>
              <strong>{t("privacy.here.mind", { size: formatBytes(view.mindBytes) })}</strong>
            </p>
            <div className={styles.bar} role="img" aria-label={t("privacy.here.bar")}>
              <span style={{ width: `${total > 0 ? (view.keptBytes / total) * 100 : 0}%` }} />
              <span style={{ width: `${total > 0 ? (view.mindBytes / total) * 100 : 0}%` }} />
            </div>
            <p className={styles.where}>
              <span className={styles.mono}>{view.dataDir}</span>
              <Button variant="quiet" onClick={() => void platform.openPath(view.dataDir)}>
                {t("privacy.here.open")}
              </Button>
            </p>
          </section>

          <section aria-labelledby="export" className={styles.block}>
            <h2 id="export" className={styles.blockTitle}>
              {t("privacy.export.title")}
            </h2>
            <p>{t("privacy.export.body")}</p>
            <div ref={exportBox}>
              <Button
                variant="secondary"
                disabled={running}
                onClick={() => void exportEverything()}
              >
                {t("privacy.export.button")}
              </Button>
            </div>
            {exporting.state === "running" ? (
              <Progress
                label={t("privacy.export.progress")}
                value={exporting.progress?.done ?? 0}
                max={exporting.progress?.total ?? 0}
                detail={
                  exporting.progress
                    ? t("progress.of", {
                        done: exporting.progress.done,
                        total: exporting.progress.total,
                      })
                    : undefined
                }
              />
            ) : null}
            <div role="status" className={styles.exportStatus}>
              {exporting.state === "running" ? t("privacy.export.started") : null}
              {exporting.state === "done" ? (
                <>
                  <p>{t("privacy.export.done", { path: exporting.report.path })}</p>
                  <p className={styles.detail}>
                    {t("privacy.export.counts", {
                      memories: exporting.report.memories,
                      pages: exporting.report.pages,
                      originals: exporting.report.originals,
                    })}
                  </p>
                </>
              ) : null}
            </div>
            {exporting.state === "done" ? (
              <div>
                <Button
                  variant="secondary"
                  onClick={() => void platform.openPath(exporting.report.path)}
                >
                  {t("privacy.export.open")}
                </Button>
              </div>
            ) : null}
            {exporting.state === "failed" ? (
              <p className={styles.problem} role="alert">
                {t("privacy.export.failed")}
              </p>
            ) : null}
          </section>

          <section className={styles.block}>
            <Toggle
              label={t("privacy.lock.label")}
              hint={t("privacy.lock.hint")}
              checked={lockOn}
              onChange={(next) => void changeLock(next)}
            />
          </section>

          <section aria-labelledby="delete" className={`${styles.block} ${styles.danger}`}>
            <h2 id="delete" className={styles.blockTitle}>
              {t("privacy.delete.title")}
            </h2>
            <p>{t("privacy.delete.body")}</p>
            <div>
              <Button variant="forget" onClick={() => setDeleting(true)}>
                {t("privacy.delete.button")}
              </Button>
            </div>
          </section>
        </div>
      </div>

      {deleting ? (
        <DeleteEverythingDialog
          title={t("privacy.delete.dialog.title")}
          body={t("privacy.delete.dialog.body")}
          confirmLabel={t("privacy.delete.confirm")}
          onClose={() => setDeleting(false)}
          onDeleted={() => {
            refresh(
              "status",
              "page",
              "pages",
              "items",
              "memories",
              "entities",
              "needsYou",
              "fading",
              "privacy",
              "setup",
            );
            setDeleting(false);
            goTo("/setup", { step: "welcome" });
          }}
        />
      ) : null}
    </div>
  );
}
