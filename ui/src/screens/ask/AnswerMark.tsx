import { hashHref } from "../../app/query";
import { PlaceIcon } from "../../components/KeepAskBar/PlaceIcon";
import { t } from "../../copy/t";
import type { AnsweredBy, LocalReason } from "../../engine/bindings";
import styles from "./Ask.module.css";

/** Why an answer was written on this computer, in words that follow "Answered on this computer". */
export function reasonText(reason: LocalReason, host: string | null): string {
  switch (reason) {
    case "chosen":
      return t("ask.reason.chosen");
    case "noInternet":
      return t("ask.reason.noInternet");
    case "couldntReach":
      return host ? t("ask.reason.couldntReach", { host }) : t("ask.reason.couldntReach.unknown");
    case "slow":
      return t("ask.reason.slow");
    case "limit":
      return t("ask.reason.limit");
    case "noCredit":
      return t("ask.reason.noCredit");
    case "keyRefused":
      return t("ask.reason.keyRefused");
    case "keptHere":
      return t("ask.reason.keptHere");
    case "problem":
      return t("ask.reason.problem");
    case "downloading":
      return t("ask.reason.downloading");
    case "turnedOff":
      return t("ask.reason.turnedOff");
    case "notSureOnline":
      return t("ask.reason.notSureOnline");
    case "changed":
      return t("ask.reason.changed");
  }
}

/**
 * Whether the answer carries a mark of its own. An answer on this computer with no reason is
 * today's line, unchanged, so it has none.
 */
export function hasMark(by: AnsweredBy | null | undefined): by is AnsweredBy {
  return !!by && !(by.type === "computer" && by.reason === null);
}

/** The mark as it is read out: where the answer was written, without the count or the link. */
export function spokenMark(by: AnsweredBy, host: string | null): string {
  return by.type === "online" ? by.host : by.reason ? reasonText(by.reason, host) : "";
}

interface Props {
  by: AnsweredBy;
  /** `done` or `notSure`. */
  phase: "done" | "notSure";
  /** How many things the answer used. */
  count: number;
  /** Your provider's host, for a reason that names it. */
  host: string | null;
  sendId: number | null;
}

/** Where this answer was written. It sits above "You asked", so a screen reader meets it first. */
export function AnswerMark({ by, phase, count, host, sendId }: Props) {
  const online = by.type === "online";
  // A question that went online and came back not sure still links to what was sent.
  const sent = online || (by.type === "computer" && by.reason === "notSureOnline");
  let text: string;
  if (by.type === "online") {
    text =
      phase === "notSure"
        ? t("ask.mark.online.notSure", { host: by.host })
        : t("ask.mark.online", { host: by.host, count });
  } else {
    const reason = by.reason ? reasonText(by.reason, host) : "";
    text =
      count > 0 ? t("ask.mark.local", { count, reason }) : t("ask.mark.local.bare", { reason });
  }
  return (
    <p className={styles.mark} data-place={online ? "online" : "computer"}>
      <PlaceIcon place={online ? "online" : "computer"} />
      <span>{text}</span>
      {sent ? (
        <>
          <span aria-hidden="true">·</span>
          <a
            href={hashHref("/settings/online/sent", {
              send: sendId === null ? null : String(sendId),
            })}
          >
            {t("ask.mark.seeSent")}
          </a>
        </>
      ) : null}
    </p>
  );
}
