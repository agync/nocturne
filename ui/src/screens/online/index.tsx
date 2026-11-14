import { useRoute } from "wouter";
import { useQueryParam } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { AdvancedConnect } from "./AdvancedConnect";
import styles from "./Online.module.css";
import { OnlinePage } from "./OnlinePage";
import Sent from "./Sent";
import { SetupFlow } from "./SetupFlow";
import { useOnline } from "./useOnline";

/** Serves #/settings/online and the pages under it. */
export default function Online() {
  const [setup] = useRoute("/settings/online/setup");
  const [sent] = useRoute("/settings/online/sent");
  const [connect] = useRoute("/settings/advanced/online");
  const step = useQueryParam("step");
  const { data, failure, refetch } = useOnline();

  if (sent) return <Sent />;
  if (!data) {
    return failure ? (
      <div className={styles.message} role="alert">
        <p>{t(failure.code === "unavailable" ? "online.unavailable" : "online.error")}</p>
        {connect ? <p>{t("advanced.errorDetail", { detail: failure.detail })}</p> : null}
        <Button variant="secondary" onClick={refetch}>
          {t("button.tryAgain")}
        </Button>
      </div>
    ) : (
      <p className={styles.message} role="status">
        {t("online.loading")}
      </p>
    );
  }
  if (connect) return <AdvancedConnect view={data} />;
  if (setup) {
    const start = step === "connect" || step === "send" ? step : null;
    return <SetupFlow view={data} start={start} />;
  }
  return <OnlinePage view={data} />;
}
