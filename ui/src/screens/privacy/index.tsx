import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { BackToAnswer } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Progress } from "../../components/Progress/Progress";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes, formatDayMonth, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { ExportProgress, ExportReport, LedgerEntry } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { useOnline } from "../online/useOnline";
import { AwaySection } from "./AwaySection";
import { BackupSection } from "./BackupSection";
import { DeleteEverythingDialog } from "./DeleteEverythingDialog";
import { OnlineSection, onlineKnown } from "./OnlineSection";
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
    remoteHelper: "privacy.ledger.remoteHelper",
    remoteOnOff: "privacy.ledger.remoteOnOff",
    onlineOnOff: "privacy.ledger.onlineOnOff",
    onlineConnected: "privacy.ledger.onlineConnected",
    modelDownload: "privacy.ledger.modelDownload",
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
  const [deleting, setDeleting] = useState(false);
  const { data: onlineView } = useOnline();
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

  // While Away from home is on the computer also talks to the phones, which nothing here measures live.
  const measured = view.sentMeasured && !view.remoteOn && !view.onlineOn;
  const online = onlineKnown(view);
  const total = view.keptBytes + view.mindBytes;

  return (
    <ScreenSheet flush className={styles.sheet}>
      <div className={styles.columns}>
        <div className={styles.proof}>
          <BackToAnswer />
          <p className={`t-section ${styles.eyebrow}`}>{t("privacy.title")}</p>
          <p className={styles.sentLabel}>
            {measured ? t("privacy.sent.label") : t("privacy.sent.label.unmeasured")}
          </p>
          <h1 className={`t-display ${styles.nothing}`}>
            {view.remoteOn && view.onlineOn
              ? t("privacy.headline.awayAndOnline")
              : view.remoteOn
                ? t("privacy.headline.awayOn")
                : view.onlineOn
                  ? t("privacy.headline.onlineOn")
                  : !measured
                    ? t("privacy.sent.unmeasured")
                    : view.sentToInternetBytes > 0
                      ? t("privacy.sent.amount", { size: formatBytes(view.sentToInternetBytes) })
                      : t("privacy.sent.nothing")}
          </h1>
          {measured ? (
            <p className={styles.watching}>
              <span className={styles.dot} aria-hidden="true" />
              {view.checkedAt ? checkedText(view.checkedAt, now) : t("privacy.watching.now")}
            </p>
          ) : (
            <p className={styles.detail}>
              {view.remoteOn
                ? t(
                    view.sentMeasured
                      ? "privacy.sent.awayOn.measured"
                      : "privacy.sent.awayOn.detail",
                    {
                      company: t(
                        view.remoteConnection === "mostPrivate"
                          ? "away.company.mostPrivate"
                          : "away.company.simple",
                      ),
                    },
                  )
                : t(
                    view.onlineOn
                      ? "privacy.sent.unmeasured.detail.online"
                      : "privacy.sent.unmeasured.detail",
                  )}
            </p>
          )}

          {online ? (
            <div className={styles.phones}>
              <span className={styles.sentLabel}>{t("privacy.online.label")}</span>
              <span className={styles.phonesAmount}>
                {view.onlineOn
                  ? t("privacy.online.amount.on", {
                      size: formatBytes(view.sentOnlineBytes),
                      count: view.onlineQuestions,
                    })
                  : t("privacy.online.amount.off", {
                      size: formatBytes(view.sentOnlineBytes),
                      day: view.onlineLastOnAt ? formatDayMonth(view.onlineLastOnAt) : "",
                    })}
              </span>
              <span className={styles.detail}>
                {t("privacy.online.counted")}{" "}
                <a href="#/settings/online/sent">{t("privacy.online.seeSent")}</a>
              </span>
            </div>
          ) : null}

          {view.remoteOn || view.sentToPhonesBytes > 0 ? (
            <div className={styles.phones}>
              <span className={styles.sentLabel}>{t("privacy.phones.label")}</span>
              <span className={styles.phonesAmount}>
                {t("privacy.phones.amount", { size: formatBytes(view.sentToPhonesBytes) })}
              </span>
              <span className={styles.detail}>{t("privacy.phones.counted")}</span>
            </div>
          ) : null}

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
                      {entry.kind === "remoteOnOff" ||
                      entry.kind === "onlineOnOff" ||
                      entry.kind === "onlineConnected"
                        ? t("privacy.ledger.entryDay", { day: formatDayMonth(entry.at) })
                        : t("privacy.ledger.entry", {
                            size: formatBytes(entry.bytes),
                            day: formatDayMonth(entry.at),
                          })}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <p className={styles.promise}>
            {t(online ? "privacy.promise.online" : "privacy.promise")}
          </p>
        </div>

        <div className={styles.controls}>
          <AwaySection view={view} />
          <OnlineSection view={view} />
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
              <Button
                variant="quiet"
                onClick={() =>
                  platform
                    .openDataFolder()
                    .catch(() => toast.show({ message: t("notice.action.failed") }))
                }
              >
                {t("privacy.here.open")}
              </Button>
            </p>
          </section>

          <section aria-labelledby="export" className={styles.block}>
            <h2 id="export" className={styles.blockTitle}>
              {t("privacy.export.title")}
            </h2>
            <p>{t(online ? "privacy.export.body.online" : "privacy.export.body")}</p>
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

          <BackupSection />

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
          online={onlineView && !onlineView.keyStorage ? null : view.onlineHost}
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
    </ScreenSheet>
  );
}
