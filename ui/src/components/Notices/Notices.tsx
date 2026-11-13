import { formatBytes, formatEta } from "../../copy/format";
import { t } from "../../copy/t";
import type { Notice, NoticeCode } from "../../engine/bindings";
import { Button } from "../Button/Button";
import styles from "./Notices.module.css";

export type NoticeHandlers = Partial<Record<NoticeCode, (notice: Notice) => void>>;

interface NoticesProps {
  notices: Notice[];
  /** What each action does. A notice without a handler shows its sentence and no button. */
  handlers?: NoticeHandlers;
}

/** E-01 to E-12: one sentence, one button. Codes never reach the screen. */
export function noticeText(notice: Notice): string {
  switch (notice.code) {
    case "diskFull":
      return notice.bytes !== null
        ? t("notice.diskFull.text", { size: formatBytes(notice.bytes) })
        : t("notice.diskFull.text.unknown");
    case "catchingUp":
      return notice.etaSeconds !== null
        ? t("notice.catchingUp.text", { eta: formatEta(notice.etaSeconds) })
        : t("notice.catchingUp.text.unknown");
    case "downloadStalled":
      return t("notice.downloadStalled.text");
    case "tierDropped":
      return t("notice.tierDropped.text");
    case "restarts":
      return t("notice.restarts.text");
    case "couldntRead":
      return t("notice.couldntRead.text");
    case "micDenied":
      return t("notice.micDenied.text");
    case "printFailed":
      return t("notice.printFailed.text");
    case "repairNeeded":
      return t("notice.repairNeeded.text");
    case "notReady":
      return t("notice.notReady.text");
    case "tidyPausedBattery":
      return t("notice.tidyPausedBattery.text");
    case "updateRolledBack":
      return t("notice.updateRolledBack.text");
    case "reReading":
      return t("notice.reReading.text");
  }
}

export function noticeAction(code: NoticeCode): string {
  switch (code) {
    case "downloadStalled":
      return t("notice.downloadStalled.action");
    case "diskFull":
      return t("notice.diskFull.action");
    case "tierDropped":
      return t("notice.tierDropped.action");
    case "restarts":
      return t("notice.restarts.action");
    case "couldntRead":
      return t("notice.couldntRead.action");
    case "micDenied":
      return t("notice.micDenied.action");
    case "printFailed":
      return t("notice.printFailed.action");
    case "catchingUp":
      return t("notice.catchingUp.action");
    case "repairNeeded":
      return t("notice.repairNeeded.action");
    case "notReady":
      return t("notice.notReady.action");
    case "tidyPausedBattery":
      return t("notice.tidyPausedBattery.action");
    case "updateRolledBack":
      return t("notice.updateRolledBack.action");
    case "reReading":
      return t("notice.reReading.action");
  }
}

export function Notices({ notices, handlers = {} }: NoticesProps) {
  if (notices.length === 0) return null;
  return (
    <section className={styles.notices} aria-label={t("notice.region")}>
      <ul className={styles.list}>
        {notices.map((notice) => {
          const handler = handlers[notice.code];
          return (
            <li key={notice.id} className={styles.notice} data-code={notice.code}>
              <p className={styles.text}>{noticeText(notice)}</p>
              {handler ? (
                <Button variant="secondary" onClick={() => handler(notice)}>
                  {noticeAction(notice.code)}
                </Button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
