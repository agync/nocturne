import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { formatTime, localDay } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { RemoteAction, RemoteEvent } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Away.module.css";
import { Frame } from "./parts";
import { useRemoteAct } from "./useRemote";
import { atText, dayHeading } from "./when";

const LIMIT = 200;
const DAY = 86_400_000;

/** What happened, in words, from the event's action alone. Never what was said or kept. */
const WHAT: Record<RemoteAction, CopyId> = {
  asked: "away.event.asked",
  keptText: "away.event.keptText",
  keptPhoto: "away.event.keptPhoto",
  keptDocument: "away.event.keptDocument",
  keptVoiceNote: "away.event.keptVoiceNote",
  answeredName: "away.event.answeredName",
  answeredQuestion: "away.event.answeredQuestion",
  tookBack: "away.event.tookBack",
  readPage: "away.event.readPage",
  openedOriginal: "away.event.openedOriginal",
  tookNewAddress: "away.event.tookNewAddress",
  allowed: "away.event.allowed",
  refused: "away.event.refused",
  removed: "away.event.removed",
  renamed: "away.event.renamed",
  disconnected: "away.event.disconnected",
  wrongCode: "away.event.wrongCode",
  limited: "away.event.limited",
  turnedOn: "away.event.turnedOn",
  turnedOff: "away.event.turnedOff",
  switchedConnection: "away.event.switchedConnection",
};

export function eventText(event: RemoteEvent): string {
  const name = event.deviceName ?? t("away.event.aPhone");
  if (event.action !== "wrongCode" && event.result === "refused" && event.action !== "refused") {
    return t("away.event.turnedAway", { name });
  }
  if (event.result === "limited") return t("away.event.slowed", { name });
  if (event.result === "failed") return t("away.event.failed", { name });
  return t(WHAT[event.action], { name, count: event.count });
}

function groupByDay(events: RemoteEvent[]): Array<{ day: string; events: RemoteEvent[] }> {
  const groups: Array<{ day: string; events: RemoteEvent[] }> = [];
  for (const event of events) {
    const day = localDay(event.at);
    const last = groups.at(-1);
    if (last && last.day === day) last.events.push(event);
    else groups.push({ day, events: [event] });
  }
  return groups;
}

/** #/settings/away/activity */
export function Activity() {
  const engine = useEngine();
  const act = useRemoteAct();
  const [dismissed, setDismissed] = useState(false);
  const query = useQuery({
    queryKey: ["remote-events"],
    queryFn: () => engine.listRemoteEvents(LIMIT),
    staleTime: 0,
  });

  if (query.isError) {
    return (
      <Frame back={{ to: "/settings/away", label: t("screen.away.title") }}>
        <div role="alert" className={styles.panel}>
          <p>{t("away.activity.error")}</p>
          <Button variant="secondary" onClick={() => void query.refetch()}>
            {t("button.tryAgain")}
          </Button>
        </div>
      </Frame>
    );
  }
  const events = query.data ?? [];
  const now = Date.now();
  const unusual = events.find(
    (e) => e.action === "wrongCode" && now - new Date(e.at).getTime() < DAY * 2,
  );

  return (
    <Frame
      back={{ to: "/settings/away", label: t("screen.away.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t("away.activity.listed.heading")}</h2>
          <p className={styles.muted}>{t("away.activity.listed.body")}</p>
          <p className={styles.muted}>{t("away.activity.kept")}</p>
          <p className={styles.muted}>
            {t("away.activity.unknown")}{" "}
            <a className="hit" href="#/settings/away/phones">
              {t("away.phones.see")}
            </a>
          </p>
        </>
      }
    >
      <h1 className="t-page-title">{t("away.activity.title")}</h1>
      {unusual && !dismissed ? (
        <div className={styles.banner} role="status">
          <p className={styles.name}>
            {t("away.activity.unusual", { count: unusual.count, when: atText(unusual.at, now) })}
          </p>
          <p>{t("away.activity.unusual.then")}</p>
          <div className={styles.buttons}>
            <Button variant="secondary" onClick={() => void act(() => engine.stopRemote(false))}>
              {t("away.activity.turnOffNow")}
            </Button>
            <Button variant="secondary" onClick={() => setDismissed(true)}>
              {t("button.ok")}
            </Button>
          </div>
        </div>
      ) : null}
      {query.isLoading ? (
        <p className={styles.muted} role="status">
          {t("away.activity.loading")}
        </p>
      ) : events.length === 0 ? (
        <p className={styles.body}>{t("away.activity.empty")}</p>
      ) : (
        groupByDay(events).map((group) => (
          <section key={group.day} className={styles.day} aria-labelledby={`day-${group.day}`}>
            <h2 id={`day-${group.day}`} className={styles.smallHeading}>
              {dayHeading(group.day, now)}
            </h2>
            <ul className={styles.events}>
              {group.events.map((event) => (
                <li key={`${event.at}-${event.action}-${event.deviceName ?? ""}`}>
                  <span>{eventText(event)}</span>
                  <span className={styles.mono}>{formatTime(event.at)}</span>
                </li>
              ))}
            </ul>
          </section>
        ))
      )}
    </Frame>
  );
}
