import { useRoute } from "wouter";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import styles from "./Away.module.css";
import { AwayPage } from "./AwayPage";
import { useRemote } from "./useRemote";

/** Serves #/settings/away and the pages under it. */
export default function Away() {
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
  return <AwayPage view={view} />;
}
