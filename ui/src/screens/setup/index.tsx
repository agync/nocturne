import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { goTo, useQueryParam } from "../../app/query";
import { ActionBarHost } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import type { RestoredView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { RestoreFlow } from "../restore";
import { Checking } from "./Checking";
import { Choosing } from "./Choosing";
import { Done } from "./Done";
import { MoveDialog } from "../away/MoveDialog";
import { GettingReady } from "./GettingReady";
import { PhoneQuestion } from "./PhoneQuestion";
import styles from "./Setup.module.css";
import { useNight } from "./useNight";
import { Welcome } from "./Welcome";

/** Serves #/setup, which draws its own window. Every phase of getting ready has a state here. */
export default function Setup() {
  useNight(true);
  const engine = useEngine();
  const step = useQueryParam("step");
  const [begun, setBegun] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [asking, setAsking] = useState(false);
  const [restored, setRestored] = useState<RestoredView | null>(null);
  const query = useQuery({ queryKey: ["setup"], queryFn: () => engine.getSetup() });
  const view = query.data;

  useEffect(() => {
    document.title = `${t("screen.setup.title")} · ${t("app.name")}`;
  }, []);

  let body: React.ReactNode;
  if (restoring) {
    body = (
      <div className={styles.card}>
        <ActionBarHost>
          <RestoreFlow
            mode="setup"
            onExit={() => setRestoring(false)}
            onContinue={(restoredView) => {
              setRestored(restoredView);
              setRestoring(false);
              setBegun(true);
              goTo("/setup");
              void query.refetch();
            }}
          />
        </ActionBarHost>
      </div>
    );
  } else if (query.isError) {
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
        // A paused or failed setup can be pointed at a different size.
        body =
          step === "choosing" && (view.phase === "paused" || view.phase === "failed") ? (
            <Choosing view={view} onStarted={() => goTo("/setup")} />
          ) : (
            <GettingReady view={view} />
          );
        break;
      case "done":
        body =
          step === "welcome" ? (
            <Welcome
              onStart={() => goTo("/setup", { step: "choosing" })}
              onRestore={() => setRestoring(true)}
            />
          ) : step === "choosing" ? (
            <Choosing view={view} onStarted={() => goTo("/setup")} />
          ) : (
            <Done restoredAt={restored?.madeAt ?? null} />
          );
        break;
      default:
        // notStarted, checking, choosing
        if (!begun && step !== "choosing" && asking) {
          body = (
            <PhoneQuestion
              onYes={() =>
                // A phone pairs over Away from home, so it is turned on first (Simple, with its
                // download asked about as anywhere else), then it hands on to adding the phone.
                void engine.getRemote().then(
                  (remote) =>
                    remote.state.type === "on"
                      ? goTo("/settings/away/add")
                      : goTo("/settings/away/setup", { phone: "1" }),
                  () => goTo("/settings/away/setup", { phone: "1" }),
                )
              }
              onNo={() => {
                setAsking(false);
                setBegun(true);
              }}
            />
          );
        } else if (!begun && step !== "choosing") {
          body = <Welcome onStart={() => setAsking(true)} onRestore={() => setRestoring(true)} />;
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
      {/* A memory moved here opens after a restart, while this computer may still be setting
          up: its message shows over setup too. */}
      <MoveDialog />
    </main>
  );
}
