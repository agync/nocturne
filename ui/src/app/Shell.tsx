import { type ReactNode, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { Dialog } from "../components/Dialog/Dialog";
import { DropOverlay } from "../components/DropOverlay/DropOverlay";
import { ErrorBoundary } from "../components/ErrorBoundary/ErrorBoundary";
import { ScreenError } from "../components/ErrorBoundary/ScreenError";
import { KeepAskBar } from "../components/KeepAskBar/KeepAskBar";
import { KeptToday } from "../components/KeptToday/KeptToday";
import { Mark, markStateFor } from "../components/Mark/Mark";
import { type NoticeHandlers, Notices } from "../components/Notices/Notices";
import { Rail } from "../components/Rail/Rail";
import { useToast } from "../components/Toast/Toast";
import { addDays, formatDay } from "../copy/format";
import { t } from "../copy/t";
import { useEngine, usePage, useRefresh, useSettings, useStatus } from "../engine/context";
import { askRequests } from "./ask-request";
import { findRoute, routes } from "./routes";
import styles from "./Shell.module.css";
import { useKeepRecording } from "./useKeepRecording";
import { useRegionCycling } from "./useRegions";
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

  useEffect(() => {
    document.title = route ? `${t(route.title)} · ${t("app.name")}` : t("app.name");
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
      } catch {
        toast.show({ message: t("bar.kept.error") });
      }
    },
    [engine, toast, refresh],
  );

  const recorder = useKeepRecording();

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
      restarts: () => navigate("/settings/advanced"),
      catchingUp: () => {
        if (today) navigate(`/pages/${addDays(today, -1)}`);
      },
      printFailed: () => {
        if (today) navigate(`/print/${today}`);
      },
    }),
    [engine, navigate, refresh, today, toast],
  );

  const bar = (
    <div className={styles.bar} data-region="keep">
      <KeepAskBar
        placement={narrow ? "bottom" : "top"}
        onKeep={keep}
        onAsk={ask}
        shortcut={settings?.shortcutAvailable ? settings.shortcut : undefined}
        onTalk={{ start: recorder.start, stop: recorder.stop }}
        recordingSeconds={recorder.seconds}
        autoFocus
      />
    </div>
  );

  const title = route ? t(route.title) : t("app.name");

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
      <div className={styles.body}>
        {narrow ? (
          <header className={styles.narrowHeader}>
            <span className={styles.narrowTitle}>
              <Mark state={markStateFor(status?.state ?? "resting")} size={18} decorative />
              <span>{title}</span>
            </span>
            <button
              type="button"
              className={styles.menuButton}
              data-region="rail"
              aria-haspopup="dialog"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen(true)}
            >
              {t("shell.menu")}
            </button>
          </header>
        ) : (
          <div className={styles.topbar}>
            {bar}
            <span className={styles.dropHint}>{t("bar.dropHint")}</span>
          </div>
        )}
        {status && status.notices.length > 0 ? (
          <Notices notices={status.notices} handlers={handlers} />
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
      {!narrow ? (
        // Last in the document, first on screen: the design's focus order ends at the rail.
        <div className={styles.railColumn}>
          <Rail />
        </div>
      ) : null}
      {menuOpen ? (
        <Dialog title={t("shell.menu.title")} edge="start" onClose={() => setMenuOpen(false)}>
          <Rail onNavigate={() => setMenuOpen(false)} />
        </Dialog>
      ) : null}
      <DropOverlay onDrop={keepFiles} />
    </div>
  );
}

export { routes };
