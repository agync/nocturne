import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { goTo, useQueryParam } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import { Checking } from "./Checking";
import { Choosing } from "./Choosing";
import { Done } from "./Done";
import { GettingReady } from "./GettingReady";
import styles from "./Setup.module.css";
import { Welcome } from "./Welcome";

/** Serves #/setup, which draws its own window. Every phase of getting ready has a state here. */
export default function Setup() {
  const engine = useEngine();
  const step = useQueryParam("step");
  const [begun, setBegun] = useState(false);
  const query = useQuery({ queryKey: ["setup"], queryFn: () => engine.getSetup() });
  const view = query.data;

  let body: React.ReactNode;
  if (query.isError) {
    body = (
      <div className={styles.message} role="alert">
        <p>{t("setup.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  } else if (!view) {
    body = (
      <p className={styles.message} role="status">
        {t("setup.loading")}
      </p>
    );
  } else {
    switch (view.phase) {
      case "downloading":
      case "paused":
      case "installing":
      case "failed":
        body = <GettingReady view={view} />;
        break;
      case "done":
        body =
          step === "welcome" ? (
            <Welcome onStart={() => goTo("/setup", { step: "choosing" })} />
          ) : step === "choosing" ? (
            <Choosing view={view} onStarted={() => goTo("/setup")} />
          ) : (
            <Done />
          );
        break;
      default:
        // notStarted, checking, choosing
        if (!begun && step !== "choosing") {
          body = <Welcome onStart={() => setBegun(true)} />;
        } else if (view.phase === "choosing") {
          body = <Choosing view={view} onStarted={() => goTo("/setup")} />;
        } else {
          body = <Checking />;
        }
    }
  }

  return (
    <main className={styles.window} aria-label={t("setup.window")}>
      {body}
    </main>
  );
}
