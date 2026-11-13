import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import styles from "./Online.module.css";
import { OnlinePage } from "./OnlinePage";
import Sent from "./Sent";
import { useOnline } from "./useOnline";

/** Serves #/settings/online and the pages under it. */
export default function Online() {
  const [setup] = useRoute("/settings/online/setup");
  const [sent] = useRoute("/settings/online/sent");
  const { data, isError, refetch } = useOnline();

  if (sent) return <Sent />;
  if (setup) return <Placeholder title="screen.online.setup.title" />;
  if (!data) {
    return isError ? (
      <div className={styles.message} role="alert">
        <p>{t("online.error")}</p>
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
  return <OnlinePage view={data} />;
}
