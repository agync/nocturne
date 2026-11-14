import { useEffect, useRef, useState } from "react";
import { useRoute } from "wouter";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { toEngineError } from "../../engine/client";
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
  const kind = query.data?.state.type;
  const lastKind = useRef(kind);
  const [announce, setAnnounce] = useState("");

  // When the connection moves on, focus goes to the new page's heading so it is read out.
  useEffect(() => {
    if (lastKind.current === kind) return;
    const seen = lastKind.current;
    lastKind.current = kind;
    if (seen === undefined) return;
    setAnnounce(kind === "on" ? t("away.announce.on") : "");
    if (document.querySelector("[role=dialog],[role=alertdialog]")) return;
    const heading = document.querySelector<HTMLElement>("main h1, h1");
    if (heading) {
      heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
    }
  }, [kind]);

  if (query.isError && !query.data) {
    return (
      <div className={styles.message} role="alert">
        <p>
          {t(toEngineError(query.error).code === "unavailable" ? "away.unavailable" : "away.error")}
        </p>
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
  const page = (() => {
    if (setup) return <SetupFlow view={view} />;
    if (add) return <AddPhone view={view} />;
    if (phones) return <Phones view={view} />;
    if (activity) return <Activity />;
    return <AwayPage view={view} />;
  })();
  return (
    <>
      <p className="sr-only" role="status">
        {announce}
      </p>
      {page}
    </>
  );
}
