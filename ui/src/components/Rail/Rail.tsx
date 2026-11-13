import { useLocation } from "wouter";
import { formatBytes, formatWeekday } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import { usePage, useStatus } from "../../engine/context";
import { Mark, markStateFor } from "../Mark/Mark";
import styles from "./Rail.module.css";

interface RailProps {
  /** Called after a link is followed, so a menu sheet can close itself. */
  onNavigate?: () => void;
}

interface Entry {
  to: string;
  label: CopyId;
  /** Other paths that keep this entry marked as the current one. */
  also?: string[];
}

const MAIN: Entry[] = [
  { to: "/today", label: "rail.today", also: ["/pages/"] },
  { to: "/needs-you", label: "rail.needsYou" },
  { to: "/memories", label: "rail.memories", also: ["/items/"] },
  { to: "/recent", label: "rail.recent" },
  { to: "/fading", label: "rail.fading" },
  { to: "/pages", label: "rail.pages" },
];

const FOOT: Entry[] = [
  { to: "/privacy", label: "rail.privacy" },
  { to: "/settings", label: "rail.settings" },
];

function isCurrent(entry: Entry, location: string): boolean {
  if (location === entry.to || location.startsWith(`${entry.to}/`)) {
    // /pages is its own entry, so /pages/:date belongs to Today.
    if (entry.to === "/pages") return location === "/pages";
    return true;
  }
  return (entry.also ?? []).some((prefix) => location.startsWith(prefix) && location !== "/pages");
}

export function Rail({ onNavigate }: RailProps) {
  const [location] = useLocation();
  const { data: status } = useStatus();
  const { data: page } = usePage();

  const link = (entry: Entry) => {
    const current = isCurrent(entry, location);
    return (
      <a
        key={entry.to}
        href={`#${entry.to}`}
        className={styles.link}
        aria-current={current ? "page" : undefined}
        onClick={onNavigate}
      >
        <span>{t(entry.label)}</span>
        {entry.to === "/today" && page ? (
          <span className={styles.mono}>{formatWeekday(page.date)}</span>
        ) : null}{" "}
        {entry.to === "/needs-you" && status && status.needsYou > 0 ? (
          <span className={styles.count}>{status.needsYou}</span>
        ) : null}
      </a>
    );
  };

  return (
    <nav className={styles.rail} aria-label={t("rail.label")} data-region="rail">
      <div className={styles.brand}>
        <Mark state="resting" size={20} decorative />
        <span className={styles.wordmark}>{t("app.name")}</span>
      </div>
      <div className={styles.links}>{MAIN.map(link)}</div>
      <div className={styles.foot}>
        {status ? (
          <section className={styles.status} aria-label={t("status.region")}>
            <span className={styles.state}>
              <Mark state={markStateFor(status.state)} size={12} decorative />
              {t(`status.state.${status.state}`)}
            </span>
            <span className={styles.internet}>
              <span className={styles.label}>{t("status.internet.label")}</span>
              <span className={styles.value}>
                {!status.sentMeasured
                  ? t("status.internet.unmeasured")
                  : status.sentToInternetBytes === 0
                    ? t("status.internet.nothing")
                    : formatBytes(status.sentToInternetBytes)}
              </span>
            </span>
          </section>
        ) : null}
        <div className={styles.secondary}>{FOOT.map(link)}</div>
      </div>
    </nav>
  );
}
