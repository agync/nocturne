import { formatLongDay, formatTimeOfDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { MindTrouble } from "../mind";

/**
 * What the phone says while the chosen service is not answering: a key it refused, no credit
 * left or the month's limit, each with the way to put it right. An unreachable service is a
 * line of its own, `UnreachableToday`.
 */
export function MindNotice({
  status,
  onFix,
}: {
  status: MindTrouble;
  /** "Check the key…", "Check again" and "Change limit" all lead to the mind's details. */
  onFix(): void;
}) {
  let title: string;
  let body: string;
  let action: string;
  if (status.state === "needsKey") {
    title = t("phone.mind.noKey.title");
    body = t("phone.mind.noKey.body", { host: status.host });
    action = t("phone.mind.noKey.action");
  } else if (status.state === "keyRefused") {
    title = t("phone.mind.trouble.refused.title");
    body = t("phone.mind.trouble.refused.body", { host: status.host });
    action = t("phone.mind.trouble.refused.action");
  } else if (status.state === "noCredit") {
    title = t("phone.mind.trouble.noCredit.title");
    body = t("phone.mind.trouble.noCredit.body");
    action = t("phone.mind.trouble.noCredit.action");
  } else if (status.state === "paused") {
    title = t("phone.mind.trouble.paused.title");
    body = t("phone.mind.trouble.paused.body", { day: formatLongDay(status.until) });
    action = t("phone.mind.trouble.paused.action");
  } else {
    return null;
  }
  return (
    <section className="ph-mind-trouble" role="status" aria-label={title}>
      <p className="ph-error-title">{title}</p>
      <p className="ph-hint ph-hint--ink2">{body}</p>
      <button type="button" className="ph-link" onClick={onFix}>
        {action}
      </button>
    </section>
  );
}

/** Under a page the service wrote, once it has stopped answering. */
export function UnreachableToday({
  status,
  written,
}: {
  status: { host: string; since: string };
  /** The page's own date. */
  written: string;
}) {
  return (
    <p className="ph-small">
      {t("phone.mind.trouble.unreachable.today", {
        day: formatLongDay(written),
        host: status.host,
        time: formatTimeOfDay(status.since),
      })}
    </p>
  );
}

/** On Today and over what Ask finds by words while this phone has no way online. */
export function Offline() {
  return (
    <p className="ph-small" role="status">
      {t("phone.mind.trouble.offline")}
    </p>
  );
}

/** Over what Ask found by words while the service is not answering. */
export function UnreachableAsk({ host }: { host: string }) {
  return <p className="ph-small">{t("phone.mind.trouble.unreachable.ask", { host })}</p>;
}
