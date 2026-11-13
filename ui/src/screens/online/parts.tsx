import type { ReactNode } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { Icon } from "../../components/Icon/Icon";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";
import styles from "./Online.module.css";
import { sentence } from "./when";

/** The page frame every Online answers screen shares: the words on the left, an aside on the right. */
export function Frame({
  children,
  aside,
  back,
}: {
  children: ReactNode;
  aside?: ReactNode;
  back: { to: string; label: string };
}) {
  return (
    <ScreenSheet>
      <BackLink to={back.to} label={back.label} />
      <div className={styles.columns}>
        <section className={styles.main}>{children}</section>
        {aside ? <aside className={styles.aside}>{aside}</aside> : null}
      </div>
    </ScreenSheet>
  );
}

/** "Your provider (api.example.com)": the only way the service is named in the interface. */
export const providerName = (host: string | null): string =>
  sentence(t("online.provider.name", { host: host ?? "" }));

export function LockNote({ children }: { children: ReactNode }) {
  return (
    <div className={styles.lockNote}>
      <Icon name="lock" size={16} />
      <p>{children}</p>
    </div>
  );
}

interface ConfirmProps {
  title: string;
  children: ReactNode;
  confirm: string;
  keep: string;
  busy?: boolean;
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

/** The worked example of what goes with a question, shared by the explainer and the last step. */
export function Example({ short = false }: { short?: boolean }) {
  return (
    <div className={styles.example}>
      <span className={styles.exampleHead}>{t("online.example.question")}</span>
      <span>{t("online.example.questionText")}</span>
      <span className={styles.exampleHead}>{t("online.example.kept")}</span>
      <ol>
        <li>
          <b>1</b>
          <span>{t("online.example.line1")}</span>
        </li>
        <li>
          <b>2</b>
          <span>{t("online.example.line2")}</span>
        </li>
      </ol>
      <span className={styles.exampleNote}>
        {t(short ? "online.example.instructionsShort" : "online.example.instructions")}
      </span>
      {short ? null : <span className={styles.exampleFoot}>{t("online.example.notSent")}</span>}
    </div>
  );
}
