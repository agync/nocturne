import { t } from "../../copy/t";
import type { ModelChoice } from "../../engine/bindings";
import { gb, JOB_TITLE } from "./jobs";
import styles from "./Models.module.css";

/** The one line under a greyed row, or under a tight one. */
export function reasonLine(choice: ModelChoice): string | null {
  const reason = choice.reason;
  if (!reason) return choice.fit === "tight" ? t("models.reason.tight") : null;
  switch (reason.type) {
    case "memory":
      return t("models.reason.memory");
    case "disk":
      return t("models.reason.disk", { short: gb(reason.shortBytes) });
    case "notAvailable":
      return t("models.reason.unavailable");
    case "needsOtherJob":
      return t("models.reason.otherJob", { job: t(JOB_TITLE[reason.job]) });
  }
}

/** The longer reason, opened in place by Why?, and a way on where there is one. */
export function Why({
  id,
  choice,
  budgetBytes,
}: {
  id: string;
  choice: ModelChoice;
  budgetBytes: number;
}) {
  const reason = choice.reason;
  if (!reason) return null;
  const budget = gb(budgetBytes);
  let text: string;
  let action: { href: string; label: string } | null = null;
  switch (reason.type) {
    case "memory":
      text = t("models.why.memory", { need: gb(choice.memoryBytes), budget });
      break;
    case "disk":
      text = t("models.why.disk", { short: gb(reason.shortBytes) });
      break;
    case "notAvailable":
      text = t("models.why.unavailable");
      break;
    case "needsOtherJob": {
      const job = t(JOB_TITLE[reason.job]);
      text = t("models.why.otherJob", { job, budget });
      action = {
        href: `#/settings/models/${reason.job}`,
        label: t("models.why.otherJobAction", { job }),
      };
      break;
    }
  }
  return (
    <section id={id} className={styles.why} aria-label={t("models.why.region")}>
      <span>{text}</span>
      {action ? (
        <a className={`${styles.linkButton} hit`} href={action.href}>
          {action.label}
        </a>
      ) : null}
    </section>
  );
}
