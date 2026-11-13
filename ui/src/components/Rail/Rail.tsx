import { useLocation } from "wouter";
import { formatBytes, formatWeekday } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { RemoteView } from "../../engine/bindings";
import { usePage, useStatus } from "../../engine/context";
import { useRemote } from "../../screens/away/useRemote";
import { Mark, markStateFor } from "../Mark/Mark";
import styles from "./Rail.module.css";

interface RailProps {
  /** Called after a link is followed, so a menu sheet can close itself. */
  onNavigate?: () => void;
  /** Show the Nocturne name and mark at the top. The menu sheet already has a title. */
  brand?: boolean;
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

const AWAY = "/settings/away";

/** The rail entry a screen belongs to, or null when it has none (Ask, Listen, not found). */
export function currentSection(location: string): CopyId | null {
  const entry = [...MAIN, ...FOOT].find((e) => isCurrent(e, location));
  return entry ? entry.label : null;
}

type Indicator = { kind: "on" | "use" | "relink" | "needs"; text: string };

/** What the rail says about Away from home, or nothing while it is off or still being set up. */
export function indicatorFor(view: RemoteView | undefined): Indicator | null {
  if (!view) return null;
  switch (view.state.type) {
    case "on": {
      const using = view.devices.filter((d) => d.active);
      if (using.length === 1 && using[0]) {
        return { kind: "use", text: t("rail.away.use", { name: using[0].name }) };
      }
      if (using.length > 1)
        return { kind: "use", text: t("rail.away.useMany", { count: using.length }) };
      return { kind: "on", text: t("rail.away.on") };
    }
    case "needsRelink":
      return { kind: "relink", text: t("rail.away.relink") };
    case "needsSignIn":
    case "problem":
      return { kind: "needs", text: t("rail.away.needs") };
    default:
      return null;
  }
}

export function Rail({ onNavigate, brand = true }: RailProps) {
  const [location] = useLocation();
  const { data: status } = useStatus();
  const { data: page } = usePage();
  const { data: remote } = useRemote();
  const away = indicatorFor(remote);
  const sentToPhones = remote?.state.type === "on" ? remote.sentToPhonesBytes : 0;

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
      {brand ? (
        <div className={styles.brand}>
          <Mark state="resting" size={20} decorative />
          <span className={styles.wordmark}>{t("app.name")}</span>
        </div>
      ) : null}
      <div className={styles.links}>{MAIN.map(link)}</div>
      <div className={styles.foot}>
        <div className={styles.away} aria-live="polite">
          {away ? (
            <a
              href={`#${AWAY}`}
              className={styles.awayLink}
              data-kind={away.kind}
              aria-live={away.kind === "relink" || away.kind === "needs" ? undefined : "off"}
              onClick={onNavigate}
            >
              <span className={styles.awayDot} aria-hidden="true" />
              <span>{away.text}</span>
            </a>
          ) : null}
        </div>
        {status ? (
          <section className={styles.status} aria-label={t("status.region")}>
            <span className={styles.state}>
              <Mark state={markStateFor(status.state)} size={12} decorative />
              {t(`status.state.${status.state}`)}
            </span>
            <span className={styles.internet}>
              <span className={styles.label}>
                {t(status.sentMeasured ? "status.internet.label" : "status.internet.uses")}
              </span>
              <span className={styles.value}>
                {!status.sentMeasured
                  ? t("status.internet.unmeasured")
                  : status.sentToInternetBytes === 0
                    ? t("status.internet.nothing")
                    : formatBytes(status.sentToInternetBytes)}
              </span>
            </span>
            {sentToPhones > 0 ? (
              <span className={styles.internet}>
                <span className={styles.label}>{t("status.phones.label")}</span>
                <span className={styles.value}>{formatBytes(sentToPhones)}</span>
              </span>
            ) : null}
          </section>
        ) : null}
        <div className={styles.secondary}>{FOOT.map(link)}</div>
      </div>
    </nav>
  );
}
