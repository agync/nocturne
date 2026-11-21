import { type ReactNode, useRef, useState } from "react";
import { tc } from "../../../components/ControlChip/copy";
import { t } from "../../../copy/t";
import type { Receipt } from "../../../engine/bindings";
import type { Connection, PairedDevice, PhoneEngine, Queue } from "../../engine";
import { whenText } from "../../screens/Home";
import { EarlierChips, PageArticle, useEarlier, usePage } from "../../screens/PageScreen";
import { Button, Icon, Page } from "../../screens/ui";
import { UpdateCard, type UpdateNeeded } from "./UpdateCard";

interface TodayProps {
  engine: PhoneEngine;
  queue: Queue;
  connection: Connection;
  device: PairedDevice;
  waiting: number;
  sentJustNow: number;
  now: number;
  update: UpdateNeeded | null;
  bar: ReactNode;
  onQueued(): void;
  onOpen(r: Receipt): void;
  onJournal(day: string | null): void;
  onRelink(kind: "scan" | "words"): void;
}

/** The page once connected; the same day again when the connection comes back. */
function TodayPage({
  engine,
  queue,
  connected,
  now,
  onQueued,
  onOpen,
  onJournal,
}: {
  engine: PhoneEngine;
  queue: Queue;
  connected: boolean;
  now(): number;
  onQueued(): void;
  onOpen(r: Receipt): void;
  onJournal(day: string | null): void;
}) {
  const [date, setDate] = useState<string | undefined>(undefined);
  const [attempt, setAttempt] = useState(0);
  const load = usePage(engine, date, attempt);
  const earlier = useEarlier(engine);
  const heading = useRef<HTMLHeadingElement>(null);
  if (load.name === "loading") {
    return (
      <p className="ph-status" role="status" aria-busy="true">
        <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
        {t("phone.page.loading")}
      </p>
    );
  }
  if (load.name === "failed") {
    return (
      <>
        <h2 className="ph-h1 ph-h1--small" role="alert">
          {t("phone.page.failed")}
        </h2>
        <Button kind="primary" onClick={() => setAttempt((n) => n + 1)}>
          {t("phone.tryAgain")}
        </Button>
      </>
    );
  }
  if (load.name === "none") {
    return (
      <>
        <h2 className="ph-h1 ph-h1--small">{t("phone.page.none")}</h2>
        <p className="ph-body">{t("phone.page.none.body")}</p>
      </>
    );
  }
  return (
    <>
      <PageArticle
        page={load.page}
        engine={engine}
        queue={queue}
        connected={connected}
        earlier={earlier}
        now={now}
        onQueued={onQueued}
        onOpen={onOpen}
        onJournal={onJournal}
        heading={heading}
        phone
      />
      <EarlierChips earlier={earlier} shown={load.page.date} onPick={setDate} />
    </>
  );
}

/**
 * Today, the first tab: the phone edition of the morning page, with last night and the
 * calendar. While the computer is away it says so, and Keep still works and waits.
 */
export function Today(p: TodayProps) {
  const kind = p.connection.kind;
  const connected = kind === "connected";
  const away = kind === "asleep" || kind === "cantConnect";
  const name =
    p.connection.kind === "connected"
      ? p.connection.hello.computerName
      : (p.device.computerName ?? t("phone.home.computer"));
  const last = whenText(p.device.lastConnectedAt, p.now);

  return (
    <Page bar={p.bar} className="ph-tabbed">
      <main className="ph-main ph-main--tight ph-today">
        {away ? (
          <div className="ph-banner" role="status">
            <Icon name="moon" size={22} />
            <div>
              <p className="ph-banner-title">
                {kind === "asleep"
                  ? t("phone.asleep.title", { name })
                  : t("phone.cantConnect.title", { name })}
              </p>
              <p className="ph-banner-body">
                {kind === "asleep" ? t("phone.asleep.body") : t("phone.cantConnect.body")}
              </p>
            </div>
          </div>
        ) : kind === "outdated" ? null : (
          <p className="ph-status" role="status">
            <span className={`ph-dot${connected ? "" : " ph-dot--breathe"}`} aria-hidden="true" />
            {connected ? t("phone.home.connected", { name }) : t("phone.home.connecting", { name })}
          </p>
        )}
        {p.sentJustNow > 0 && connected ? (
          <p className="ph-hint ph-hint--ink2" role="status">
            {t("phone.home.sent", { count: p.sentJustNow })}
          </p>
        ) : null}
        {p.update ? <UpdateCard update={p.update} /> : null}
        {connected && !p.update ? (
          <TodayPage
            engine={p.engine}
            queue={p.queue}
            connected={connected}
            now={() => p.now}
            onQueued={p.onQueued}
            onOpen={p.onOpen}
            onJournal={p.onJournal}
          />
        ) : null}
        {!connected || p.update ? (
          <div className="ph-stack" style={{ gap: 8 }}>
            {kind === "connecting" ? null : <p className="ph-body">{t("phone.app.today.away")}</p>}
            {p.waiting > 0 || away ? (
              <p className="ph-status">
                <span className="ph-dot ph-dot--ember" aria-hidden="true" />
                {p.waiting > 0
                  ? t("phone.app.today.waiting", {
                      count: p.waiting,
                      last: last ? t("phone.asleep.last", { when: last }) : t("phone.asleep.never"),
                    })
                  : last
                    ? t("phone.asleep.last", { when: last })
                    : t("phone.asleep.never")}
              </p>
            ) : null}
          </div>
        ) : null}
        {kind === "asleep" && p.device.connection !== "mostPrivate" ? (
          <section className="ph-card" aria-labelledby="new-address-title">
            <h2 id="new-address-title" className="ph-card-title">
              {t("phone.asleep.address.title")}
            </h2>
            <p className="ph-hint ph-hint--ink2">{tc("phone.address.where")}</p>
            <Button kind="quiet" onClick={() => p.onRelink("scan")}>
              <Icon name="camera" size={20} />
              {t("phone.address.scan")}
            </Button>
            <Button kind="quiet" onClick={() => p.onRelink("words")}>
              {t("phone.address.words")}
            </Button>
          </section>
        ) : null}
        {connected ? (
          <div className="ph-privacy-line" style={{ border: 0, paddingTop: 14 }}>
            <Icon name="lock" size={16} />
            <p className="ph-hint ph-hint--ink2">{t("phone.home.private")}</p>
          </div>
        ) : null}
      </main>
    </Page>
  );
}
