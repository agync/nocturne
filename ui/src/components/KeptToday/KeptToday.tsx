import { addDays } from "../../copy/format";
import { t } from "../../copy/t";
import type { ItemKind, ItemStatus, ItemView } from "../../engine/bindings";
import { useItems, usePage, useStatus } from "../../engine/context";
import styles from "./KeptToday.module.css";

function startOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d).toISOString();
}

function kindLabel(item: ItemView): string {
  const kind: ItemKind = item.kind;
  switch (kind) {
    case "audio":
      return t("kind.audio");
    case "image":
      return t("kind.image");
    case "text":
      return item.source === "paste" ? t("kind.pasted") : t("kind.text");
    case "web":
      return t("kind.web");
    case "document":
      return t("kind.document");
    case "checkin":
      return t("kind.checkin");
  }
}

const STATUS_COPY = {
  kept: "kept.status.kept",
  reading: "kept.status.reading",
  understood: "kept.status.understood",
  partlyUnderstood: "kept.status.partlyUnderstood",
  couldntRead: "kept.status.couldntRead",
  needsYou: "kept.status.needsYou",
  unconfirmed: "kept.status.unconfirmed",
} as const satisfies Record<ItemStatus, Parameters<typeof t>[0]>;

const SHOWN = 5;

/** What was kept today, with a word for where each thing is. */
export function KeptToday() {
  const { data: page } = usePage();
  const { data: status } = useStatus();
  const since = page ? startOf(page.date) : null;
  const until = page ? startOf(addDays(page.date, 1)) : null;
  const { data: items } = useItems({ since, status: null, limit: null }, since !== null);
  const today = (items ?? []).filter(
    (i) => until === null || new Date(i.keptAt).getTime() < new Date(until).getTime(),
  );

  return (
    <aside className={styles.kept} aria-label={t("kept.region")}>
      <h2 className={styles.heading}>{t("kept.heading", { count: today.length })}</h2>
      {today.length === 0 ? (
        <p className={styles.empty}>{t("kept.empty")}</p>
      ) : (
        <ul className={styles.list}>
          {today.slice(0, SHOWN).map((item) => (
            <li key={item.id} className={styles.item}>
              <a className={styles.title} href={`#/items/${item.id}`}>
                {item.kind === "audio" ? t("kept.audioTitle", { title: item.title }) : item.title}
              </a>
              <span className={styles.meta} data-status={item.status}>
                {item.status === "reading" ? (
                  <span className={styles.dot} aria-hidden="true" />
                ) : null}
                {kindLabel(item)} · {t(STATUS_COPY[item.status])}
              </span>
            </li>
          ))}
        </ul>
      )}
      <a className={styles.all} href="#/recent">
        {t("kept.all")}
      </a>
      <p className={styles.tonight}>
        {status?.nextTidy
          ? t("kept.tonight", { time: status.nextTidy })
          : t("kept.tonight.unknown")}
      </p>
    </aside>
  );
}
