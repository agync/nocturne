import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { t } from "../../copy/t";
import styles from "./Online.module.css";
import { Frame } from "./parts";
import { StepAside, StepLabel } from "./SetupBits";

/** Step 2: the service's details live under Advanced details; this step sends the person there. */
export function ServiceStep({
  host,
  onNext,
  onLeave,
}: {
  host: string | null;
  onNext: () => void;
  onLeave: () => void;
}) {
  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <StepAside
          heading={t("online.service.aside.heading")}
          paragraphs={[t("online.service.aside.apart"), t("online.service.aside.check")]}
        />
      }
    >
      <StepLabel step={2} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.service.title")}
      </h1>
      <p className={styles.body}>
        {host ? t("online.service.ready", { host }) : t("online.service.lead")}
      </p>
      <p className={styles.sub}>{tc("online.service.where")}</p>
      <div className={styles.footer}>
        {host ? (
          <>
            <Button variant="primary" onClick={onNext}>
              {t("online.continue")}
            </Button>
            <Button
              variant="secondary"
              onClick={() => goTo("/settings/advanced/online", { from: "online" })}
            >
              {t("online.service.open")}
            </Button>
          </>
        ) : (
          <Button
            variant="primary"
            onClick={() => goTo("/settings/advanced/online", { from: "online" })}
          >
            {t("online.service.open")}
          </Button>
        )}
        <Button variant="quiet" onClick={onLeave}>
          {t("online.notNow")}
        </Button>
      </div>
    </Frame>
  );
}
