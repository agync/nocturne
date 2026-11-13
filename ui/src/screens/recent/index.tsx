import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { StatusLabel } from "../../components/StatusLabel/StatusLabel";
import { addDays } from "../../copy/format";
import { t } from "../../copy/t";
import type { ItemStatus, ItemView } from "../../engine/bindings";
import { useEngine, useItems, usePage, useRefresh, useStatus } from "../../engine/context";
import { actionFor, needsUnlockedCopy, type RowAction, subline, thumbCode } from "./describe";
import styles from "./Recent.module.css";

/** Local midnight at the start of `day`, as an instant. */
function startOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d).toISOString();
}

const ACTION_COPY: Record<RowAction["type"], Parameters<typeof t>[0]> = {
  readNow: "recent.action.readNow",
  open: "recent.action.open",
  which: "recent.action.which",
  confirm: "recent.action.confirm",
  retry: "button.tryAgain",
};

const STATUS_WORD: Record<ItemStatus, Parameters<typeof t>[0]> = {
  kept: "item.status.kept",
  reading: "item.status.reading",
  understood: "item.status.understood",
  partlyUnderstood: "item.status.partlyUnderstood",
  couldntRead: "item.status.couldntRead",
  needsYou: "item.status.needsYou",
  unconfirmed: "item.status.unconfirmed",
};

/** What was kept today and yesterday, and where each thing is on its way to being understood. */
export default function Recent() {
  const engine = useEngine();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const { data: page } = usePage();
  const { data: status } = useStatus();
  const today = page?.date;
  const since = useMemo(() => (today ? startOf(addDays(today, -1)) : null), [today]);
  const items = useItems({ since, status: null, limit: null }, since !== null);
  const [announcement, setAnnouncement] = useState("");
  const [failed, setFailed] = useState(false);
  const list = useRef<HTMLUListElement>(null);
  const known = useRef<Map<string, ItemStatus> | null>(null);

  const rows = items.data ?? [];

  // Say what changed, politely, but only for things just kept and for the row the person is on.
  useEffect(() => {
    if (!items.data) return;
    const before = known.current;
    const now = new Map(items.data.map((i) => [i.id, i.status]));
    known.current = now;
    if (before === null) return;
    const focused = document.activeElement?.closest<HTMLElement>("[data-item]")?.dataset.item;
    const said: string[] = [];
    for (const item of items.data) {
      const was = before.get(item.id);
      if (was === undefined) {
        said.push(t("recent.announce.new", { title: item.title }));
      } else if (was !== item.status && item.id === focused) {
        said.push(
          t("recent.announce.changed", { title: item.title, status: t(STATUS_WORD[item.status]) }),
        );
      }
    }
    if (said.length > 0) setAnnouncement(said.join(" "));
  }, [items.data]);

  const run = async (item: ItemView, action: RowAction, row: HTMLElement | null) => {
    switch (action.type) {
      case "open":
        navigate(`/items/${item.id}`);
        return;
      case "which":
      case "confirm":
        navigate("/needs-you");
        return;
      case "readNow":
      case "retry":
        try {
          setFailed(false);
          if (action.type === "readNow") await engine.readNow(item.id);
          else await engine.retryItem(item.id);
          // The button goes away once reading starts; the row keeps the focus so nothing is lost.
          row?.focus();
          refresh("items", "status");
        } catch {
          setFailed(true);
        }
    }
  };

  if (items.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("recent.error")}</p>
        <button type="button" className={styles.retry} onClick={() => void items.refetch()}>
          {t("button.tryAgain")}
        </button>
      </div>
    );
  }
  if (!items.data) {
    return (
      <p className={styles.message} role="status">
        {t("recent.loading")}
      </p>
    );
  }

  const waiting =
    status?.waitingToRead ??
    rows.filter((i) => i.status === "kept" || i.status === "reading").length;

  return (
    <ScreenSheet>
      <header className={styles.header}>
        <div className={styles.headerText}>
          <p className={styles.eyebrow}>{t("recent.eyebrow")}</p>
          <h1 className="t-page-title">{t("recent.title")}</h1>
        </div>
        <p className={styles.waiting}>{t("recent.waiting", { count: waiting })}</p>
      </header>

      {rows.length === 0 ? (
        <div className={styles.empty}>
          <p className={styles.emptyTitle}>{t("recent.empty.title")}</p>
          <p className={styles.emptyBody}>{t("recent.empty.body")}</p>
        </div>
      ) : (
        <ul ref={list} className={styles.list} aria-label={t("recent.list")}>
          {rows.map((item) => {
            const action = actionFor(item);
            const locked = needsUnlockedCopy(item);
            return (
              <li
                key={item.id}
                className={styles.row}
                data-item={item.id}
                data-status={item.status}
                data-kind={item.kind}
                tabIndex={-1}
              >
                <span className={styles.thumb} data-kind={item.kind} aria-hidden="true">
                  {thumbCode(item)}
                </span>
                <span className={styles.text}>
                  <span className={styles.name}>{item.title}</span>
                  <span className={styles.sub}>{subline(item, today ?? "")}</span>
                </span>
                <span className={styles.state}>
                  <StatusLabel status={item.status} tonight={item.status === "kept"} />
                </span>
                <span className={styles.act}>
                  {action ? (
                    <Button
                      variant="quiet"
                      aria-label={t("recent.action.aria", {
                        action: t(ACTION_COPY[action.type]),
                        title: item.title,
                      })}
                      onClick={(event) => void run(item, action, event.currentTarget.closest("li"))}
                    >
                      {t(ACTION_COPY[action.type])}
                    </Button>
                  ) : locked ? (
                    <span className={styles.locked}>{t("recent.locked")}</span>
                  ) : null}
                </span>
              </li>
            );
          })}
        </ul>
      )}

      {failed ? (
        <p className={styles.failed} role="alert">
          {t("recent.failed")}
        </p>
      ) : null}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {announcement}
      </div>
    </ScreenSheet>
  );
}
