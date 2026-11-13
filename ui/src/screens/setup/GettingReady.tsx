import { useState } from "react";
import { goTo } from "../../app/query";
import { useKeepRecording } from "../../app/useKeepRecording";
import { Button } from "../../components/Button/Button";
import { Progress } from "../../components/Progress/Progress";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes, formatEta } from "../../copy/format";
import { t } from "../../copy/t";
import type { SetupView, Tier } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { Defaults } from "./Defaults";
import styles from "./Setup.module.css";
import { useArrivalFocus } from "./useArrivalFocus";

/** What is wrong, in one sentence and one button. A failed setup always has a way on. */
function Problem({ view, onAgain }: { view: SetupView; onAgain: () => void }) {
  const error = view.error ?? (view.phase === "failed" ? "engineFailed" : null);
  if (!error) return null;
  const sentence = {
    offline: t("notice.downloadStalled.text"),
    diskFull:
      view.neededBytes !== null
        ? t("notice.diskFull.text", { size: formatBytes(view.neededBytes) })
        : t("notice.diskFull.text.unknown"),
    notEnoughMemory: t("setup.error.notEnoughMemory"),
    engineFailed: t("setup.error.engineFailed"),
  }[error];
  const button = {
    offline: t("notice.downloadStalled.action"),
    diskFull: t("button.tryAgain"),
    notEnoughMemory: t("setup.error.chooseSmaller"),
    engineFailed: t("button.tryAgain"),
  }[error];
  return (
    <div className={styles.problemBox} role="alert">
      <p>{sentence}</p>
      <Button
        variant="secondary"
        onClick={error === "notEnoughMemory" ? () => goTo("/setup", { step: "choosing" }) : onAgain}
      >
        {button}
      </Button>
    </div>
  );
}

const PAUSED = {
  person: "setup.paused.person",
  offline: "setup.paused.offline",
  battery: "setup.paused.battery",
} as const;

/** H3: capture already works while the mind downloads. */
export function GettingReady({ view }: { view: SetupView }) {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const refresh = useRefresh();
  const heading = useArrivalFocus();
  const recorder = useKeepRecording();
  const [typing, setTyping] = useState(false);
  const [note, setNote] = useState("");
  const [announce, setAnnounce] = useState("");

  const run = async (work: () => Promise<unknown>, message?: string) => {
    try {
      await work();
      refresh("setup");
      if (message) setAnnounce(message);
    } catch {
      toast.show({ message: t("setup.action.failed") });
    }
  };

  const retry = () =>
    run(() =>
      view.phase === "failed" &&
      (view.chosen ?? view.detected) &&
      (view.chosen ?? view.detected) !== "unsupported"
        ? engine.startSetup((view.chosen ?? view.detected) as Tier)
        : engine.resumeSetup(),
    );

  const pickFiles = async () => {
    const paths = await platform.pickFiles();
    if (paths.length === 0) return;
    try {
      await engine.keepFiles(paths, "drop");
      refresh("items", "status", "page");
      toast.show({ message: t("capture.files", { count: paths.length }) });
    } catch {
      toast.show({ message: t("setup.keep.failed") });
    }
  };

  const keepNote = async () => {
    const text = note.trim();
    if (!text) return;
    try {
      await engine.keepText(text, "quickCapture");
      refresh("items", "status", "page");
      setNote("");
      setTyping(false);
      toast.show({ message: t("capture.kept.text") });
    } catch {
      toast.show({ message: t("setup.keep.failed") });
    }
  };

  const paused = view.phase === "paused";
  const installing = view.phase === "installing";
  const hasProblem = view.error !== null || view.phase === "failed";
  const detail =
    view.etaSeconds !== null && !paused
      ? t("setup.progress.detail", {
          done: formatBytes(view.downloadedBytes),
          total: formatBytes(view.totalBytes),
          eta: formatEta(view.etaSeconds),
        })
      : t("setup.progress.detail.noEta", {
          done: formatBytes(view.downloadedBytes),
          total: formatBytes(view.totalBytes),
        });

  return (
    <section className={`${styles.card} ${styles.wide}`} aria-labelledby="ready">
      <div className={styles.main}>
        <p className={`t-section ${styles.eyebrow}`}>{t("setup.step.two")}</p>
        <h1 ref={heading} tabIndex={-1} id="ready" className="t-page-title">
          {t("setup.ready.title")}
        </h1>

        <div className={styles.keep}>
          <h2 className={styles.keepTitle}>{t("setup.keep.title")}</h2>
          <p>{t("setup.keep.body")}</p>
          <div className={styles.keepButtons}>
            <Button variant="primary" onClick={() => void pickFiles()}>
              {t("setup.keep.file")}
            </Button>
            <Button
              variant="secondary"
              data-recording={recorder.seconds !== null}
              onPointerDown={() => recorder.start()}
              onPointerUp={() => recorder.stop()}
              onKeyDown={(event) => {
                if ((event.key === " " || event.key === "Enter") && !event.repeat) {
                  event.preventDefault();
                  recorder.start();
                }
              }}
              onKeyUp={(event) => {
                if (event.key === " " || event.key === "Enter") recorder.stop();
              }}
            >
              {recorder.seconds !== null
                ? t("bar.listening", {
                    time: `${Math.floor(recorder.seconds / 60)}:${String(recorder.seconds % 60).padStart(2, "0")}`,
                  })
                : t("bar.talk")}
            </Button>
            <Button variant="secondary" aria-expanded={typing} onClick={() => setTyping(!typing)}>
              {t("setup.keep.type")}
            </Button>
          </div>
          {typing ? (
            <div className={styles.note}>
              <label htmlFor="note" className={styles.noteLabel}>
                {t("setup.keep.note.label")}
              </label>
              <textarea
                id="note"
                className={styles.textarea}
                rows={3}
                value={note}
                // biome-ignore lint/a11y/noAutofocus: the person just chose to type a note
                autoFocus
                onChange={(event) => setNote(event.target.value)}
              />
              <div>
                <Button
                  variant="secondary"
                  disabled={note.trim() === ""}
                  onClick={() => void keepNote()}
                >
                  {t("setup.keep.note.save")}
                </Button>
              </div>
            </div>
          ) : null}
        </div>

        <div className={styles.progress}>
          <Problem view={view} onAgain={() => void retry()} />
          {installing && !hasProblem ? (
            <>
              <Progress
                label={t("setup.progress.installing")}
                value={view.totalBytes}
                max={view.totalBytes}
              />
              <p className={styles.safe}>{t("setup.installing")}</p>
            </>
          ) : (
            <>
              <Progress
                label={t("setup.progress.label")}
                value={view.downloadedBytes}
                max={view.totalBytes}
                detail={detail}
                paused={paused}
              />
              <div className={styles.safeRow}>
                <span className={styles.safe}>
                  {paused && view.paused ? t(PAUSED[view.paused]) : t("setup.progress.safe")}
                </span>
                {view.phase === "downloading" || (paused && view.paused === "person") ? (
                  <Button
                    variant="quiet"
                    onClick={() =>
                      void run(
                        () => (paused ? engine.resumeSetup() : engine.pauseSetup()),
                        paused ? t("setup.resume") : t("setup.paused.person"),
                      )
                    }
                  >
                    {paused ? t("setup.resume") : t("setup.pause")}
                  </Button>
                ) : null}
              </div>
            </>
          )}
          <p className="sr-only" role="status">
            {announce}
          </p>
        </div>
      </div>
      <Defaults />
    </section>
  );
}
