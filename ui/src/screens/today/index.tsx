import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useRoute } from "wouter";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { PageSheet, type SheetAction } from "../../components/PageSheet/PageSheet";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes, isDay } from "../../copy/format";
import { paceDays } from "../../copy/pace";
import { t } from "../../copy/t";
import type { PageLine, Period, WhichResolved } from "../../engine/bindings";
import {
  useEngine,
  usePage,
  usePages,
  useRefresh,
  useSettings,
  useStatus,
} from "../../engine/context";
import { NotFound } from "../NotFound";
import { applyOverrides, NO_OVERRIDES, type Overrides } from "./optimistic";
import styles from "./Today.module.css";

/** "Yesterday, looked back on ›": the one quiet line to the look back the night wrote. */
function lookedBack(period: Period): string {
  switch (period.kind) {
    case "day":
      return t("journal.page.yesterday");
    case "week":
      return t("journal.page.week");
    case "month":
      return t("journal.page.month", {
        month: new Intl.DateTimeFormat("en-GB", { month: "long" }).format(
          new Date(`${period.start}T12:00:00`),
        ),
      });
    case "year":
      return t("journal.page.year", { year: period.start.slice(0, 4) });
  }
}

/** Serves #/today and #/pages/:date. */
export default function Today() {
  const [, params] = useRoute("/pages/:date");
  const date = params?.date;
  // A new day starts with a clean slate of just-now changes.
  if (date !== undefined && !isDay(date)) return <NotFound />;
  return <TodayPage key={date ?? "today"} date={date} />;
}

function TodayPage({ date }: { date: string | undefined }) {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const pageQuery = usePage(date);
  const { data: pages } = usePages();
  const { data: settings } = useSettings();
  const { data: status } = useStatus();
  const todayQuery = usePage();
  const client = useQueryClient();
  const [overrides, setOverrides] = useState<Overrides>(NO_OVERRIDES);
  const [resolved, setResolved] = useState<Record<string, WhichResolved>>({});
  const [forgetting, setForgetting] = useState<PageLine | null>(null);
  const inflight = useRef(0);
  const forgottenLine = useRef<string | null>(null);

  const page = useMemo(
    () => (pageQuery.data ? applyOverrides(pageQuery.data, overrides) : undefined),
    [pageQuery.data, overrides],
  );

  const previousDate = useMemo(() => {
    if (!page || !pages) return null;
    return (
      pages
        .map((p) => p.date)
        .filter((d) => d < page.date)
        .sort()
        .at(-1) ?? null
    );
  }, [page, pages]);

  // Just-now changes stay on screen until fresh data has arrived, then give way to it.
  const settle = useCallback(async () => {
    await Promise.all(
      ["page", "pages", "status", "needsYou"].map((key) =>
        client.invalidateQueries({ queryKey: [key] }),
      ),
    );
    if (inflight.current === 0) setOverrides(NO_OVERRIDES);
  }, [client]);

  const run = useCallback(
    async (apply: () => void, revert: () => void, work: () => Promise<unknown>) => {
      apply();
      inflight.current += 1;
      try {
        await work();
        inflight.current -= 1;
        await settle();
      } catch {
        inflight.current -= 1;
        revert();
        toast.show({ message: t("page.action.failed") });
      }
    },
    [settle, toast],
  );

  /** "Mark as done" waits out its toast: line id -> the memory to mark. */
  const pendingDone = useRef(new Map<string, string>());

  const commitDone = useCallback(
    (id: string) => {
      const memoryId = pendingDone.current.get(id);
      if (memoryId === undefined) return;
      pendingDone.current.delete(id);
      // The held press still counts as in flight until `run` has taken over.
      void run(
        () => {},
        () => setOverrides((o) => ({ ...o, done: o.done.filter((x) => x !== id) })),
        () => engine.markDone(memoryId),
      );
      inflight.current -= 1;
    },
    [engine, run],
  );

  // A press that is still waiting for its toast to end must not be lost.
  useEffect(() => {
    const flush = () => {
      for (const id of [...pendingDone.current.keys()]) commitDone(id);
    };
    window.addEventListener("pagehide", flush);
    return () => {
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, [commitDone]);

  const onAction = useCallback(
    (action: SheetAction) => {
      if (!page) return;
      const set = (fn: (o: Overrides) => Overrides) => setOverrides(fn);
      switch (action.type) {
        case "done": {
          const id = action.line.id;
          const memoryId = action.line.memoryIds[0];
          if (!memoryId) {
            toast.show({ message: t("page.action.failed") });
            break;
          }
          pendingDone.current.set(id, memoryId);
          inflight.current += 1;
          set((o) => ({ ...o, done: [...o.done, id] }));
          toast.show({
            key: `done:${id}`,
            message: t("page.done.toast"),
            action: {
              label: t("page.done.undo"),
              onAction: () => {
                if (!pendingDone.current.delete(id)) return;
                inflight.current -= 1;
                set((o) => ({ ...o, done: o.done.filter((x) => x !== id) }));
              },
            },
            onExpire: () => commitDone(id),
          });
          break;
        }
        case "which": {
          const id = action.line.id;
          const conflictId = action.line.conflictId;
          void run(
            () => set((o) => ({ ...o, chosen: { ...o.chosen, [id]: action.choice.memoryId } })),
            () =>
              set((o) => {
                const { [id]: _removed, ...rest } = o.chosen;
                return { ...o, chosen: rest };
              }),
            async () => {
              if (!conflictId) throw new Error("This line cannot be resolved");
              const result = await engine.resolveWhich(conflictId, action.choice.memoryId);
              setResolved((r) => ({ ...r, [id]: result }));
            },
          );
          break;
        }
        case "confirm": {
          const id = action.line.id;
          const memoryId = action.line.memoryIds[0];
          void run(
            () => set((o) => ({ ...o, done: [...o.done, id] })),
            () => set((o) => ({ ...o, done: o.done.filter((x) => x !== id) })),
            async () => {
              if (!memoryId) throw new Error("This line has nothing to confirm");
              await engine.confirmMemory(memoryId, true);
            },
          );
          break;
        }
        case "forget":
          setForgetting(action.line);
          break;
        case "bringBack": {
          const id = action.line.id;
          void run(
            () => set((o) => ({ ...o, restored: [...o.restored, id] })),
            () => set((o) => ({ ...o, restored: o.restored.filter((x) => x !== id) })),
            () => engine.restore(action.line.memoryIds),
          );
          break;
        }
        case "answer":
          void run(
            () => set((o) => ({ ...o, answer: action.choice })),
            () => set((o) => ({ ...o, answer: null })),
            async () => {
              await engine.answerQuestion(page.date, action.choice);
              refresh("settings");
            },
          );
          break;
        case "open":
          navigate(`/items/${action.receipt.itemId}`);
          break;
        case "edition":
          if (action.edition === "audio") navigate(`/listen/${page.date}`);
          if (action.edition === "print") navigate(`/print/${page.date}`);
          break;
      }
    },
    [page, engine, navigate, refresh, run, toast, commitDone],
  );

  const forgetDialog = forgetting ? (
    <ForgetDialog
      memoryIds={forgetting.memoryIds}
      onClose={() => {
        const id = forgottenLine.current;
        forgottenLine.current = null;
        setForgetting(null);
        if (id) {
          // The button that opened the dialog is gone, so the line's own result takes focus.
          setTimeout(
            () => document.querySelector<HTMLElement>(`[data-forgotten-line="${id}"]`)?.focus(),
            0,
          );
        }
      }}
      onDone={() => {
        const { id } = forgetting;
        forgottenLine.current = id;
        setOverrides((o) => ({ ...o, forgotten: [...o.forgotten, id] }));
        void settle();
        refresh("items", "memories");
      }}
    />
  ) : null;

  if (pageQuery.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("page.error")}</p>
        <button type="button" className={styles.retry} onClick={() => void pageQuery.refetch()}>
          {t("page.error.retry")}
        </button>
      </div>
    );
  }
  if (!page) {
    return (
      <p className={styles.message} role="status">
        {t("page.loading")}
      </p>
    );
  }

  // Only a measured figure is worded; unmeasured reads as what is true, not as "nothing".
  const sent = status?.sentMeasured
    ? status.sentToInternetBytes > 0
      ? formatBytes(status.sentToInternetBytes)
      : t("page.footer.sent.nothing")
    : undefined;

  return (
    <div className={styles.frame}>
      <div className={styles.card}>
        <PageSheet
          page={page}
          edition="screen"
          onAction={onAction}
          previousDate={previousDate}
          pageTime={settings?.pageTime}
          sentToInternet={sent}
          progress={status?.tidy ?? null}
          live={!date || date === todayQuery.data?.date}
          resolved={resolved}
          fadeDays={paceDays(settings?.lettingGo).fade}
          after={
            page.journalLine ? (
              <p className={styles.looked}>
                <a href={`#/journal/look-back/${page.journalLine.kind}/${page.journalLine.start}`}>
                  {lookedBack(page.journalLine)}
                </a>
              </p>
            ) : null
          }
        />
      </div>
      {forgetDialog}
    </div>
  );
}
