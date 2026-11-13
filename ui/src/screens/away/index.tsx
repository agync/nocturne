import { useRoute } from "wouter";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { Activity } from "./Activity";
import { AddPhone } from "./AddPhone";
import styles from "./Away.module.css";
import { AwayPage } from "./AwayPage";
import { Phones } from "./Phones";
import { SetupFlow } from "./SetupFlow";
import { useRemote } from "./useRemote";

/** Serves #/settings/away and the pages under it. */
export default function Away() {
  const [setup] = useRoute("/settings/away/setup");
  const [add] = useRoute("/settings/away/add");
  const [phones] = useRoute("/settings/away/phones");
  const [activity] = useRoute("/settings/away/activity");
  const query = useRemote();

  if (query.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("away.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  const view = query.data;
  if (!view) {
    return (
      <p className={styles.message} role="status">
        {t("away.loading")}
      </p>
    );
  }
  if (setup) return <SetupFlow view={view} />;
  if (add) return <AddPhone view={view} />;
  if (phones) return <Phones view={view} />;
  if (activity) return <Activity />;
  return <AwayPage view={view} />;
}
