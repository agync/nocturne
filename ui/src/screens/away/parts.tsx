import type { ReactNode } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Dialog } from "../../components/Dialog/Dialog";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";
import type { DeviceView, RemoteConnection } from "../../engine/bindings";
import styles from "./Away.module.css";
import { whenText } from "./when";

/** The page frame every Away from home screen shares: the words on the left, an aside on the right. */
export function Frame({
  children,
  aside,
  narrow = false,
  back,
}: {
  children: ReactNode;
  aside?: ReactNode;
  narrow?: boolean;
  back: { to: string; label: string };
}) {
  return (
    <ScreenSheet>
      <BackLink to={back.to} label={back.label} />
      <div className={styles.columns} data-narrow={narrow}>
        <section className={styles.main}>{children}</section>
        {aside ? <aside className={styles.aside}>{aside}</aside> : null}
      </div>
    </ScreenSheet>
  );
}

/** What the company in the middle can see, in the same words wherever a connection is described. */
export function SeesText({ connection }: { connection: RemoteConnection }) {
  return (
    <p className={styles.body}>
      {tc(connection === "simple" ? "away.sees.simple" : "away.sees.mostPrivate")}
    </p>
  );
}

/** What a phone can and cannot do, as two short lists. */
export function PhoneFacts() {
  return (
    <>
      <h2 className={styles.label}>{t("away.can.heading")}</h2>
      <ul className={styles.rules}>
        <li>{t("away.can.ask")}</li>
        <li>{t("away.can.keep")}</li>
        <li>{t("away.can.read")}</li>
      </ul>
      <h2 className={styles.label}>{t("away.cant.heading")}</h2>
      <ul className={styles.rules}>
        <li>{t("away.cant.forget")}</li>
        <li>{t("away.cant.export")}</li>
        <li>{t("away.cant.settings")}</li>
      </ul>
      <p className={styles.sub}>{t("away.cant.here")}</p>
    </>
  );
}

export const companyOf = (connection: RemoteConnection) =>
  connection === "simple" ? t("away.company.simple") : t("away.company.mostPrivate");

export const connectionName = (connection: RemoteConnection) =>
  connection === "simple" ? t("away.connection.simple") : t("away.connection.mostPrivate");

/** What a phone is doing, in words: the dot is never the only sign. */
export function DeviceStatus({ device, now }: { device: DeviceView; now?: number }) {
  if (device.active) {
    return (
      <span className={styles.statusLine}>
        <span className={styles.dot} aria-hidden="true" />
        {t("away.device.using")}
      </span>
    );
  }
  if (device.needsAddress) return <span>{t("away.device.needsAddress")}</span>;
  if (device.lastUsedAt) {
    return <span>{t("away.device.lastUsed", { when: whenText(device.lastUsedAt, now) })}</span>;
  }
  return <span>{t("away.device.unused")}</span>;
}

interface ConfirmProps {
  title: string;
  children: ReactNode;
  confirm: string;
  keep: string;
  busy?: boolean;
  /** Turning something off or removing it for good: the confirm button is ember, like Delete. */
  destructive?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

/** A question with a safe first choice: focus starts on the button that changes nothing. */
export function Confirm({
  title,
  children,
  confirm,
  keep,
  busy,
  destructive,
  onConfirm,
  onClose,
}: ConfirmProps) {
  return (
    <Dialog title={title} onClose={onClose} locked={busy}>
      <div className={styles.confirm}>
        {children}
        <div className={styles.buttons}>
          <Button variant="secondary" onClick={onClose} disabled={busy}>
            {keep}
          </Button>
          <Button
            variant={destructive ? "forget" : "primary"}
            confirming={destructive}
            onClick={onConfirm}
            disabled={busy}
          >
            {confirm}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
