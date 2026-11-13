import { useCallback, useMemo, useState } from "react";
import { useLocation, useRoute } from "wouter";
import { PageSheet, type SheetAction } from "../../components/PageSheet/PageSheet";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes, isDay } from "../../copy/format";
import { t } from "../../copy/t";
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
  const [overrides, setOverrides] = useState<Overrides>(NO_OVERRIDES);

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

  const settle = useCallback(() => refresh("page", "pages", "status", "needsYou"), [refresh]);

  const run = useCallback(
    async (apply: () => void, revert: () => void, work: () => Promise<unknown>) => {
      apply();
      try {
        await work();
        settle();
      } catch {
        revert();
        toast.show({ message: t("page.action.failed") });
      }
    },
    [settle, toast],
  );

  const onAction = useCallback(
    (action: SheetAction) => {
      if (!page) return;
      const set = (fn: (o: Overrides) => Overrides) => setOverrides(fn);
      switch (action.type) {
        case "done": {
          const id = action.line.id;
          const memoryId = action.line.memoryIds[0];
          void run(
            () => set((o) => ({ ...o, done: [...o.done, id] })),
            () => set((o) => ({ ...o, done: o.done.filter((x) => x !== id) })),
            () => (memoryId ? engine.markDone(memoryId) : Promise.resolve()),
          );
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
            () =>
              conflictId
                ? engine.resolveWhich(conflictId, action.choice.memoryId)
                : Promise.resolve(),
          );
          break;
        }
        case "confirm": {
          const id = action.line.id;
          const memoryId = action.line.memoryIds[0];
          void run(
            () => set((o) => ({ ...o, done: [...o.done, id] })),
            () => set((o) => ({ ...o, done: o.done.filter((x) => x !== id) })),
            () => (memoryId ? engine.confirmMemory(memoryId, action.confirmed) : Promise.resolve()),
          );
          break;
        }
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
    [page, engine, navigate, refresh, run],
  );

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

  const sent =
    status?.sentMeasured && status.sentToInternetBytes > 0
      ? formatBytes(status.sentToInternetBytes)
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
        />
      </div>
    </div>
  );
}
