import { useEffect, useRef, useState } from "react";
import { tc } from "../../components/ControlChip/copy";
import { formatDay, formatTimeOfDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { Connection, PairedDevice } from "../engine";
import { BackBar, Bar, Button, Icon, type IconName, Page } from "./ui";

/** "07:48", or "Mon 10 Nov, 18:22" when it was not today. */
export function whenText(at: number | null, now: number): string | null {
  if (at === null) return null;
  const then = new Date(at);
  const today = new Date(now);
  const sameDay = then.toDateString() === today.toDateString();
  return sameDay
    ? formatTimeOfDay(then, "locale")
    : `${formatDay(then)}, ${formatTimeOfDay(then, "locale")}`;
}

function computerName(device: PairedDevice, connection: Connection): string {
  if (connection.kind === "connected") return connection.hello.computerName;
  return device.computerName ?? t("phone.home.computer");
}

interface HomeProps {
  device: PairedDevice;
  connection: Connection;
  waiting: number;
  headline: string | null;
  needsPage: boolean;
  sentJustNow: number;
  now: number;
  onAsk(): void;
  onKeep(): void;
  onPage(): void;
  onJournal(): void;
  onMenu(): void;
  onRelink(kind: "scan" | "words"): void;
}

function Tile({
  icon,
  title,
  hint,
  off,
  onPress,
  tag,
  children,
}: {
  icon: IconName;
  title: string;
  hint?: string;
  off?: boolean;
  onPress: () => void;
  tag?: string;
  children?: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className={`ph-tile${off ? " ph-tile--off" : ""}${children ? " ph-tile--stack" : ""}`}
      aria-disabled={off ? true : undefined}
      onClick={off ? undefined : onPress}
    >
      <span
        className={children ? "ph-tile-head" : undefined}
        style={children ? undefined : { display: "contents" }}
      >
        <span className="ph-ic" aria-hidden="true">
          <Icon name={icon} />
        </span>
        <span className="ph-tile-text">
          <span className="ph-tt">
            <span>{title}</span>
            {tag && <span className="ph-tag">{tag}</span>}
          </span>
          {hint && <span className="ph-th">{hint}</span>}
        </span>
      </span>
      {children}
    </button>
  );
}

/** P1: connected, connecting, asleep and "can't connect". */
export function Home(p: HomeProps) {
  const name = computerName(p.device, p.connection);
  const kind = p.connection.kind;
  const down = kind === "asleep" || kind === "cantConnect";
  const connected = kind === "connected";
  const last = whenText(p.device.lastConnectedAt, p.now);
  return (
    <Page bar={<Bar onMenu={p.onMenu} heading />}>
      <main className="ph-main ph-main--tight" style={{ paddingTop: down ? 20 : 22 }}>
        {down ? (
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
              {p.device.connection === "mostPrivate" && (
                <p className="ph-banner-extra">{t("phone.asleep.mostPrivate")}</p>
              )}
            </div>
          </div>
        ) : (
          <p className="ph-status" role="status">
            <span className={`ph-dot${connected ? "" : " ph-dot--breathe"}`} aria-hidden="true" />
            {connected ? t("phone.home.connected", { name }) : t("phone.home.connecting", { name })}
          </p>
        )}
        {p.sentJustNow > 0 && connected && (
          <p className="ph-hint ph-hint--ink2" role="status">
            {t("phone.home.sent", { count: p.sentJustNow })}
          </p>
        )}

        {down ? (
          <>
            <Tile
              icon="plus"
              title={t("phone.home.keep.title")}
              hint={t("phone.asleep.keep.hint", { count: p.waiting })}
              tag={p.waiting > 0 ? t("phone.asleep.waiting", { count: p.waiting }) : undefined}
              onPress={p.onKeep}
            />
            <Tile
              icon="ask"
              title={t("phone.home.ask.title")}
              hint={t("phone.asleep.ask.hint")}
              off
              onPress={p.onAsk}
            />
            <Tile
              icon="page"
              title={t("phone.home.page.title")}
              hint={t("phone.asleep.page.hint")}
              off
              onPress={p.onPage}
            />
            <Tile
              icon="book"
              title={t("phone.journal.tile.title")}
              hint={t("phone.journal.tile.asleep")}
              off
              onPress={p.onJournal}
            />
            {kind === "asleep" && p.device.connection !== "mostPrivate" && (
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
            )}
          </>
        ) : (
          <>
            <Tile
              icon="ask"
              title={t("phone.home.ask.title")}
              hint={t("phone.home.ask.hint")}
              off={!connected}
              onPress={p.onAsk}
            />
            <Tile
              icon="plus"
              title={t("phone.home.keep.title")}
              hint={t("phone.home.keep.hint")}
              tag={p.waiting > 0 ? t("phone.asleep.waiting", { count: p.waiting }) : undefined}
              onPress={p.onKeep}
            />
            <Tile
              icon="page"
              title={t("phone.home.page.title")}
              off={!connected}
              onPress={p.onPage}
            >
              <p className="ph-tile-headline">
                {p.headline ?? (p.needsPage ? t("phone.home.page.none") : " ")}
              </p>
            </Tile>
            <Tile
              icon="book"
              title={t("phone.journal.tile.title")}
              hint={t("phone.journal.tile.hint")}
              off={!connected}
              onPress={p.onJournal}
            />
          </>
        )}

        <div className="ph-spacer" />
        {down ? (
          <p className="ph-rc" style={{ color: "var(--ink2)", padding: "0 2px" }}>
            {last ? t("phone.asleep.last", { when: last }) : t("phone.asleep.never")}
          </p>
        ) : (
          <div className="ph-privacy-line" style={{ border: 0, paddingTop: 14 }}>
            <Icon name="lock" size={16} />
            <p className="ph-hint ph-hint--ink2">{t("phone.home.private")}</p>
          </div>
        )}
      </main>
    </Page>
  );
}

/** P-Offline-off: Away from home was turned off on the computer. */
export function TurnedOff({
  device,
  waiting,
  now,
  onKeep,
  onMenu,
}: {
  device: PairedDevice;
  waiting: number;
  now: number;
  onKeep(): void;
  onMenu(): void;
}) {
  const last = whenText(device.lastConnectedAt, now);
  return (
    <Page bar={<Bar onMenu={onMenu} />}>
      <main className="ph-main">
        <div className="ph-live" role="status">
          <span className="ph-icon-box" aria-hidden="true">
            <Icon name="power" size={24} />
          </span>
          <h1 className="ph-h1">{t("phone.off.title")}</h1>
          <p className="ph-body">
            {t("phone.off.body", { name: device.computerName ?? t("phone.home.computer") })}
          </p>
          {last && <span className="ph-rc">{t("phone.off.last", { when: last })}</span>}
        </div>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <p className="ph-hint ph-hint--ink2">{t("phone.off.keep.hint")}</p>
          <Button onClick={onKeep}>
            {t("phone.off.keep")}
            {waiting > 0 ? ` · ${t("phone.asleep.waiting", { count: waiting })}` : ""}
          </Button>
        </div>
      </main>
    </Page>
  );
}

/** The phone's app is older than the computer wants: the key stays, and a new pairing replaces it. */
export function Outdated({ onScan, onType }: { onScan(): void; onType(): void }) {
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <div className="ph-live" role="alert" style={{ flex: "none" }}>
          <span className="ph-icon-box ph-icon-box--ember" aria-hidden="true">
            <Icon name="unlink" size={24} />
          </span>
          <h1 className="ph-h1">{t("phone.outdated.title")}</h1>
          <p className="ph-body">{t("phone.outdated.body")}</p>
        </div>
        <div className="ph-spacer ph-stack">
          <Button kind="primary" onClick={onScan}>
            <Icon name="camera" size={20} />
            {t("phone.welcome.scan")}
          </Button>
          <Button kind="quiet" onClick={onType}>
            {t("phone.welcome.type")}
          </Button>
        </div>
      </main>
    </Page>
  );
}

/** P-Removed: only ever shown after the computer's sealed word that this phone was removed. */
export function Removed({ waiting, onClear }: { waiting: number; onClear(): Promise<void> }) {
  const [confirming, setConfirming] = useState(false);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus();
  }, []);
  return (
    <Page bar={<Bar />}>
      <main className="ph-main">
        <div className="ph-live" role="alert" style={{ flex: "none" }}>
          <span className="ph-icon-box" aria-hidden="true">
            <Icon name="unlink" size={24} />
          </span>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {t("phone.removed.title")}
          </h1>
          <p className="ph-body">{t("phone.removed.body")}</p>
        </div>
        <div className="ph-spacer" />
        {waiting > 0 && !confirming && (
          <div className="ph-card" style={{ gap: 12, padding: 18 }}>
            <p style={{ margin: 0, fontSize: "0.9375rem", lineHeight: 1.5 }}>
              <strong style={{ fontWeight: 500 }}>
                {t("phone.removed.waiting", { count: waiting })}
              </strong>{" "}
              {t("phone.removed.waitingBody", { count: waiting })}
            </p>
            <Button kind="quiet" onClick={() => setConfirming(true)}>
              {t("phone.removed.clear")}
            </Button>
          </div>
        )}
        {waiting > 0 && confirming && (
          <section
            className="ph-card"
            aria-labelledby="clear-title"
            style={{ gap: 12, padding: 18 }}
          >
            <h2 id="clear-title" className="ph-card-title">
              {t("phone.removed.confirm.title")}
            </h2>
            <p className="ph-hint ph-hint--ink2">
              {t("phone.removed.confirm.body", { count: waiting })}
            </p>
            <Button kind="danger" onClick={() => void onClear().then(() => setConfirming(false))}>
              {t("phone.removed.confirm.yes")}
            </Button>
            <Button kind="quiet" onClick={() => setConfirming(false)}>
              {t("phone.removed.confirm.no")}
            </Button>
          </section>
        )}
      </main>
    </Page>
  );
}

/** P5: this phone's details, and Disconnect. */
export function Sheet({
  device,
  connection,
  waiting,
  version,
  onClose,
  onDisconnect,
}: {
  device: PairedDevice;
  connection: Connection;
  waiting: number;
  version: string;
  onClose(): void;
  onDisconnect(): Promise<void>;
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const done = useRef<HTMLButtonElement>(null);
  const returnTo = useRef<Element | null>(null);

  useEffect(() => {
    returnTo.current = document.activeElement;
    done.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      (returnTo.current as HTMLElement | null)?.focus?.();
    };
  }, [onClose]);

  const name = computerName(device, connection);
  const mostPrivate =
    (connection.kind === "connected" ? connection.hello.connection : device.connection) ===
    "mostPrivate";
  const shownVersion = connection.kind === "connected" ? connection.hello.version : device.version;
  return (
    <>
      <div className="ph-scrim" aria-hidden="true" onClick={onClose} />
      <section className="ph-sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        <span className="ph-grip" aria-hidden="true" />
        <div className="ph-row-between">
          <h2 id="sheet-title">
            {confirming ? t("phone.sheet.confirm.title") : t("phone.sheet.title")}
          </h2>
          <button type="button" ref={done} className="ph-ib ph-ib--accent" onClick={onClose}>
            {t("phone.done")}
          </button>
        </div>
        {confirming ? (
          <>
            <p className="ph-body">{t("phone.sheet.confirm.body")}</p>
            {waiting > 0 && (
              <p className="ph-hint ph-hint--ink2">
                {t("phone.sheet.confirm.waiting", { count: waiting })}
              </p>
            )}
            <Button
              kind="danger"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void onDisconnect();
              }}
            >
              {t("phone.sheet.confirm.yes")}
            </Button>
            <Button kind="quiet" onClick={() => setConfirming(false)}>
              {t("phone.sheet.confirm.no")}
            </Button>
          </>
        ) : (
          <>
            <dl className="ph-defs">
              <div className="ph-def">
                <dt>{t("phone.sheet.name")}</dt>
                <dd>
                  {device.deviceName}
                  <small>{t("phone.sheet.rename")}</small>
                </dd>
              </div>
              <div className="ph-def">
                <dt>{t("phone.sheet.computer")}</dt>
                <dd>{name}</dd>
              </div>
              <div className="ph-def">
                <dt>{t("phone.sheet.connection")}</dt>
                <dd>{mostPrivate ? t("phone.sheet.mostPrivate") : t("phone.sheet.simple")}</dd>
              </div>
              <div className="ph-def">
                <dt>{t("phone.sheet.version")}</dt>
                <dd className="ph-mono">
                  {t("phone.sheet.versionValue", { version: shownVersion ?? version })}
                </dd>
              </div>
            </dl>
            <p className="ph-hint ph-hint--ink2" style={{ fontSize: "0.875rem" }}>
              {t("phone.sheet.can")}
            </p>
            <Button kind="danger" onClick={() => setConfirming(true)}>
              {t("phone.sheet.disconnect")}
            </Button>
            <p className="ph-hint" style={{ textAlign: "center", fontSize: "0.8125rem" }}>
              {t("phone.sheet.disconnect.hint")}
            </p>
          </>
        )}
      </section>
    </>
  );
}

export { BackBar };
