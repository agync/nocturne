import type { ReactNode } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { t } from "../../copy/t";
import type { DeviceView, RemoteConnection } from "../../engine/bindings";
import styles from "./Away.module.css";
import { whenText } from "./when";

/** The page frame every Away from home screen shares: the words on the left, an aside on the right. */
export function Frame({
  children,
  aside,
  narrow = false,
}: {
  children: ReactNode;
  aside?: ReactNode;
  narrow?: boolean;
}) {
  return (
    <div className={styles.frame}>
      <div className={styles.columns} data-narrow={narrow}>
        <section className={styles.main}>{children}</section>
        {aside ? <aside className={styles.aside}>{aside}</aside> : null}
      </div>
    </div>
  );
}

export function Back({ to, label }: { to: string; label: string }) {
  return (
    <a className={styles.back} href={`#${to}`}>
      <span aria-hidden="true">←</span>
      {label}
    </a>
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
          <Button variant="primary" onClick={onConfirm} disabled={busy}>
            {confirm}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
