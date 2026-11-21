import { useInfiniteQuery } from "@tanstack/react-query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { formatLongDay, formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type { CameInRow, CameInTag } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Phones.module.css";
import { cameInWhen, usePhoneSources } from "./usePhoneSources";

const PAGE = 30;

const TAG: Record<CameInTag, Parameters<typeof t>[0]> = {
  new: "phones.cameIn.new",
  changed: "phones.cameIn.changed",
  removed: "phones.cameIn.removed",
};

/** The entry's own day, with its time and calendar when the computer says them. */
function entryLine(row: CameInRow): string {
  const day = formatLongDay(row.start ?? row.day);
  let when = day;
  if (row.allDay) when = t("phones.cameIn.allDay", { day });
  else if (row.start) when = t("phones.cameIn.day", { day, time: formatTime12(row.start) });
  return row.calendar ? t("phones.cameIn.with", { when, calendar: row.calendar }) : when;
}

/** #/settings/phones/came-in/:deviceId/:source: what one phone sent, newest change first. */
export function CameIn({ deviceId, source }: { deviceId: string; source: string }) {
  const engine = useEngine();
  const sources = usePhoneSources();
  const phone = sources.phones?.find((p) => p.deviceId === deviceId);
  const known = source === "calendar" || source === "health";
  const query = useInfiniteQuery({
    queryKey: ["phoneCameIn", deviceId, source],
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) =>
      (await engine.listCameIn(
        deviceId,
        source === "health" ? "health" : "calendar",
        pageParam,
        PAGE,
      )) as CameInRow[],
    getNextPageParam: (last) =>
      last.length === PAGE ? (last[last.length - 1]?.cameInAt ?? undefined) : undefined,
    enabled: known && phone !== undefined,
  });

  const back = <BackLink to="/settings/phones" label={t("screen.phones.title")} />;
  if (sources.isSuccess && (!phone || !known)) {
    return (
      <ScreenSheet>
        {back}
        <p className={styles.lede}>{t("phones.cameIn.unknown")}</p>
      </ScreenSheet>
    );
  }
  if (!phone) {
    return (
      <ScreenSheet>
        {back}
        <p className={styles.muted} role="status">
          {t("phones.loading")}
        </p>
      </ScreenSheet>
    );
  }

  const rows = query.data?.pages.flat() ?? [];
  return (
    <ScreenSheet>
      {back}
      <div className={styles.main}>
        <h1 className="t-page-title">{t("phones.cameIn.title", { phone: phone.name })}</h1>
        <p className={styles.lede}>{t("phones.cameIn.lead")}</p>
        {query.isError ? (
          <div className={styles.message} role="alert">
            <p>{t("phones.cameIn.error")}</p>
            <Button variant="secondary" onClick={() => void query.refetch()}>
              {t("button.tryAgain")}
            </Button>
          </div>
        ) : query.isPending ? (
          <p className={styles.muted} role="status">
            {t("phones.loading")}
          </p>
        ) : rows.length === 0 ? (
          <p className={styles.lede}>{t("phones.cameIn.empty", { phone: phone.name })}</p>
        ) : (
          <ul className={styles.cameList} aria-label={t("phones.cameIn.list")}>
            {rows.map((row) => (
              <li key={`${row.itemId}-${row.cameInAt}`} className={styles.cameRow}>
                <span className={styles.cameWhen}>{cameInWhen(row.cameInAt, "list")}</span>
                <span className={styles.cameText}>
                  <a className={styles.cameTitle} href={`#/items/${row.itemId}`}>
                    {row.title}
                  </a>
                  <span className={styles.cameDay}>{entryLine(row)}</span>
                </span>
                <span className={styles.tag} data-tag={row.tag}>
                  {t(TAG[row.tag])}
                </span>
              </li>
            ))}
          </ul>
        )}
        {query.hasNextPage ? (
          <div className={styles.more}>
            <Button
              variant="quiet"
              flush
              disabled={query.isFetchingNextPage}
              onClick={() => void query.fetchNextPage()}
            >
              {t("phones.cameIn.more")}
            </Button>
          </div>
        ) : null}
      </div>
    </ScreenSheet>
  );
}
