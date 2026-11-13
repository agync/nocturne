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
  /** "Not now" for the notices that offer a choice. Shown beside the action. */
  dismissals?: NoticeHandlers;
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
      return notice.bytes !== null
        ? t("notice.tierDropped.text", { size: formatBytes(notice.bytes) })
        : t("notice.tierDropped.text.unknown");
    case "tierUnfinished":
      return notice.bytes !== null
        ? t("notice.tierUnfinished.text", { size: formatBytes(notice.bytes) })
        : t("notice.tierUnfinished.text.unknown");
    case "restarts":
      return t("notice.restarts.text");
    case "couldntRead":
      return t("notice.couldntRead.text");
    case "micDenied":
      return t("notice.micDenied.text");
    case "printFailed":
      // Nothing prints by itself in this version, so this notice is never shown.
      return "";
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
    case "onlineKeyRefused":
      return t("notice.onlineKeyRefused.text");
    case "onlineNoCredit":
      return t("notice.onlineNoCredit.text");
    case "onlineLimit":
      return t("notice.onlineLimit.text");
    case "modelDownloadStopped":
      return t("notice.modelDownloadStopped.text");
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
    case "tierUnfinished":
      return t("notice.tierUnfinished.action");
    case "restarts":
      return t("notice.restarts.action");
    case "couldntRead":
      return t("notice.couldntRead.action");
    case "micDenied":
      return t("notice.micDenied.action");
    case "printFailed":
      return "";
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
    case "onlineKeyRefused":
      return t("notice.onlineKeyRefused.action");
    case "onlineNoCredit":
      return t("notice.onlineNoCredit.action");
    case "onlineLimit":
      return t("notice.onlineLimit.action");
    case "modelDownloadStopped":
      return t("notice.modelDownloadStopped.action");
  }
}

/** The label of the way to turn an offer down, for the notices that make one. */
export function noticeDismiss(code: NoticeCode): string | null {
  if (code === "tierDropped") return t("notice.tierDropped.dismiss");
  if (code === "tierUnfinished") return t("notice.tierUnfinished.dismiss");
  return null;
}

/** Problems that stop something working are announced at once; the rest wait their turn. */
const URGENT: ReadonlySet<NoticeCode> = new Set<NoticeCode>([
  "diskFull",
  "downloadStalled",
  "repairNeeded",
]);

export function Notices({ notices, handlers = {}, dismissals = {} }: NoticesProps) {
  const render = (list: Notice[]) =>
    list.length === 0 ? null : (
      <ul className={styles.list} aria-label={t("notice.region")}>
        {list.map((notice) => {
          const handler = handlers[notice.code];
          const dismiss = dismissals[notice.code];
          const dismissLabel = noticeDismiss(notice.code);
          return (
            <li key={notice.id} className={styles.notice} data-code={notice.code}>
              <p className={styles.text}>{noticeText(notice)}</p>
              {handler || (dismiss && dismissLabel) ? (
                <div className={styles.actions}>
                  {handler ? (
                    <Button variant="secondary" onClick={() => handler(notice)}>
                      {noticeAction(notice.code)}
                    </Button>
                  ) : null}
                  {dismiss && dismissLabel ? (
                    <Button variant="quiet" onClick={() => dismiss(notice)}>
                      {dismissLabel}
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    );
  const shown = notices.filter((n) => n.code !== "printFailed");
  const urgent = shown.filter((n) => URGENT.has(n.code));
  const calm = shown.filter((n) => !URGENT.has(n.code));
  // The two live regions stay mounted even when empty, so the first notice is announced.
  return (
    <div className={styles.notices} data-empty={shown.length === 0}>
      <div role="alert">{render(urgent)}</div>
      <div role="status" aria-live="polite">
        {render(calm)}
      </div>
    </div>
  );
}
