import { type ReactNode, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { Dialog } from "../components/Dialog/Dialog";
import { DropOverlay } from "../components/DropOverlay/DropOverlay";
import { ErrorBoundary } from "../components/ErrorBoundary/ErrorBoundary";
import { ScreenError } from "../components/ErrorBoundary/ScreenError";
import { Icon } from "../components/Icon/Icon";
import { KeepAskBar } from "../components/KeepAskBar/KeepAskBar";
import { KeptToday } from "../components/KeptToday/KeptToday";
import { Mark, markStateFor } from "../components/Mark/Mark";
import { type NoticeHandlers, Notices } from "../components/Notices/Notices";
import { currentSection, Rail } from "../components/Rail/Rail";
import { useToast } from "../components/Toast/Toast";
import { addDays, formatDay } from "../copy/format";
import { t } from "../copy/t";
import { EngineError } from "../engine/client";
import { useEngine, usePage, useRefresh, useSettings, useStatus } from "../engine/context";
import { usePlatform } from "../platform/context";
import { askRequests } from "./ask-request";
import { findRoute, routes } from "./routes";
import styles from "./Shell.module.css";
import { useKeepRecording } from "./useKeepRecording";
import { failureCopy, recordingState } from "./useRecorder";
import { useBarShortcut, useRegionCycling } from "./useRegions";
import { useShellSize } from "./useShellSize";

interface ShellProps {
  children?: ReactNode;
}

export function Shell({ children }: ShellProps) {
  const [location, navigate] = useLocation();
  const route = findRoute(location);
  const root = useRef<HTMLDivElement>(null);
  const main = useRef<HTMLElement>(null);
  const size = useShellSize(root);
  const narrow = size === "narrow";
  const [menuOpen, setMenuOpen] = useState(false);
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const { data: status } = useStatus();
  const { data: settings } = useSettings();
  const { data: page } = usePage();

  useRegionCycling(root);
  useBarShortcut(root);

  const platform = usePlatform();
  useEffect(
    () =>
      platform.onAskHandOff((question) => {
        askRequests.request(question);
        navigate("/ask");
      }),
    [platform, navigate],
  );

  useEffect(
    () =>
      platform.onOpenInMain((path) => {
        // Only the screens of this window: never quick capture, setup or print, whose close or
        // exit behaviour would act on the main window.
        if (typeof path !== "string" || !path.startsWith("/") || !findRoute(path)?.shell) return;
        window.location.hash = `#${path}`;
      }),
    [platform],
  );

  useEffect(() => {
    document.title = `${t(route ? route.title : "screen.notFound.title")} · ${t("app.name")}`;
  }, [route]);

  const firstRoute = useRef(true);
  useEffect(() => {
    if (firstRoute.current) {
      firstRoute.current = false;
      return;
    }
    main.current?.focus({ preventScroll: true });
    main.current?.scrollTo?.({ top: 0 });
  }, [location]);

  useEffect(() => {
    if (!narrow) setMenuOpen(false);
  }, [narrow]);

  const keep = useCallback(
    async (text: string) => {
      try {
        const kept = await engine.keepText(text, "quickCapture");
        if (kept.type === "duplicate") {
          toast.show({
            message: t("capture.duplicate", {
              kind: t(`kind.${kept.item.kind === "text" ? "text" : kept.item.kind}.spoken`),
              day: formatDay(kept.item.keptAt),
            }),
          });
        } else {
          toast.show({ lead: t("capture.kept.lead"), message: t("capture.kept.later") });
        }
        refresh("items", "status", "page");
      } catch (error) {
        toast.show({ message: t("bar.kept.error") });
        throw error;
      }
    },
    [engine, toast, refresh],
  );

  const ask = useCallback(
    (text: string) => {
      askRequests.request(text);
      navigate("/ask");
    },
    [navigate],
  );

  const keepFiles = useCallback(
    async (paths: string[]) => {
      try {
        await engine.keepFiles(paths, "drop");
        toast.show({ message: t("capture.files", { count: paths.length }) });
        refresh("items", "status", "page");
      } catch (error) {
        toast.show({ message: t("bar.kept.error") });
        // The bar words a kind of file that cannot be kept itself.
        if (error instanceof EngineError && /cannot be kept/.test(error.detail)) throw error;
      }
    },
    [engine, toast, refresh],
  );

  const recorder = useKeepRecording();

  // One line in the log, so a support question about the microphone can be answered.
  useEffect(() => {
    void engine.reportRecording(recordingState()).catch(() => undefined);
  }, [engine]);

  const today = page?.date;
  const handlers: NoticeHandlers = useMemo(
    () => ({
      couldntRead: (n) => {
        if (n.itemId) {
          engine
            .retryItem(n.itemId)
            .then(() => refresh("items", "status"))
            .catch(() => toast.show({ message: t("notice.action.failed") }));
        }
      },
      downloadStalled: () => {
        engine.resumeSetup().catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      diskFull: () => navigate("/setup"),
      tierDropped: () => {
        engine
          .getTierOffer()
          .then(async (tier) => {
            if (tier) {
              await engine.startSetup(tier);
              toast.show({ message: t("notice.tierDropped.started") });
            }
            refresh("status");
          })
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      tierUnfinished: () => {
        engine
          .getUnfinishedTier()
          .then(async (tier) => {
            if (tier) {
              await engine.startSetup(tier);
              toast.show({ message: t("notice.tierUnfinished.started") });
            }
            refresh("status");
          })
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      tidyPausedBattery: () => {
        engine
          .tidyAnyway()
          .then(() => refresh("status"))
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      restarts: () => {
        engine
          .retryEngine()
          .then(() => refresh("status"))
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      catchingUp: () => {
        if (today) navigate(`/pages/${addDays(today, -1)}`);
      },
      micDenied: () => {
        engine.openSoundSettings().catch(() => toast.show({ message: t("notice.action.failed") }));
      },
    }),
    [engine, navigate, refresh, today, toast],
  );

  const dismissals: NoticeHandlers = useMemo(
    () => ({
      tierDropped: () => {
        engine
          .declineTierOffer()
          .then(() => refresh("status"))
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
      tierUnfinished: () => {
        engine
          .declineUnfinishedTier()
          .then(() => refresh("status"))
          .catch(() => toast.show({ message: t("notice.action.failed") }));
      },
    }),
    [engine, refresh, toast],
  );

  const bar = (
    <div className={styles.bar} data-region="keep">
      <KeepAskBar
        placement={narrow ? "bottom" : "top"}
        onKeep={keep}
        onAsk={ask}
        shortcut={settings?.shortcutAvailable ? settings.shortcut : undefined}
        onFiles={keepFiles}
        onTalk={{ start: recorder.start, stop: recorder.stop }}
        recordingSeconds={recorder.seconds}
        talkUnavailable={recorder.unavailable ? t(failureCopy(recorder.unavailable)) : undefined}
        focusShortcut
      />
    </div>
  );

  const title = t(route ? route.title : "screen.notFound.title");
  const sectionId = currentSection(location);
  const section = sectionId ? t(sectionId) : title;

  return (
    <div
      ref={root}
      className={styles.shell}
      data-size={size}
      style={narrow ? ({ "--toast-bottom": "5.5rem" } as React.CSSProperties) : undefined}
    >
      <button type="button" className={styles.skip} onClick={() => main.current?.focus()}>
        {t("shell.skip")}
      </button>
      {!narrow ? (
        // First in the document and on screen: the main navigation comes first in keyboard order.
        // The skip link above it jumps past it to the page.
        <div className={styles.railColumn}>
          <Rail />
        </div>
      ) : null}
      <div className={styles.body}>
        {narrow ? (
          <header className={styles.narrowHeader}>
            <button
              type="button"
              className={styles.titleButton}
              data-region="rail"
              aria-haspopup="dialog"
              aria-expanded={menuOpen}
              aria-label={t("shell.menu.named", { section })}
              onClick={() => setMenuOpen(true)}
            >
              <Mark state={markStateFor(status?.state ?? "resting")} size={18} decorative />
              <span className={styles.titleText}>{section}</span>
              <span className={styles.chevron} aria-hidden="true">
                <Icon name="chevron-down" size={16} />
              </span>
            </button>
          </header>
        ) : (
          <div className={styles.topbar}>
            {bar}
            <span className={styles.dropHint}>{t("bar.dropHint")}</span>
          </div>
        )}
        {status && status.notices.length > 0 ? (
          <Notices notices={status.notices} handlers={handlers} dismissals={dismissals} />
        ) : null}
        <div className={styles.content} data-size={size}>
          <main
            ref={main}
            id="main"
            className={styles.main}
            tabIndex={-1}
            aria-label={title}
            data-region="main"
          >
            <ErrorBoundary resetKey={location} fallback={<ScreenError />}>
              <Suspense fallback={<p className={styles.loading}>{t("screen.loading")}</p>}>
                {children}
              </Suspense>
            </ErrorBoundary>
          </main>
          {!narrow ? (
            <div className={styles.aside}>
              <KeptToday />
            </div>
          ) : null}
        </div>
        {narrow ? <div className={styles.bottomBar}>{bar}</div> : null}
      </div>
      {menuOpen ? (
        <Dialog title={t("shell.menu.title")} edge="start" onClose={() => setMenuOpen(false)}>
          <Rail brand={false} onNavigate={() => setMenuOpen(false)} />
        </Dialog>
      ) : null}
      <DropOverlay onDrop={keepFiles} />
    </div>
  );
}

export { routes };
