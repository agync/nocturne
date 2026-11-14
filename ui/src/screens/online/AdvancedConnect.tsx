import { useEffect, useRef, useState } from "react";
import { goTo, useQueryParam } from "../../app/query";
import { t } from "../../copy/t";
import type { CheckOutcome, OnlineView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { Connect, type ConnectForm } from "./Connect";
import { keepPassedCheck } from "./connection";

/**
 * #/settings/advanced/online: where the service's details are entered and checked. A check that
 * passes is held by the computer until the plain steps go on to use it; leaving any other way
 * tells the computer to drop it.
 */
export function AdvancedConnect({ view }: { view: OnlineView }) {
  const engine = useEngine();
  const from = useQueryParam("from");
  const [form, setForm] = useState<ConnectForm>({
    baseUrl: view.baseUrl ?? "",
    key: "",
    model: view.model ?? "",
  });
  const [outcome, setOutcome] = useState<CheckOutcome | null>(null);
  const checked = useRef(false);
  const handedOver = useRef(false);

  useEffect(
    () => () => {
      if (checked.current && !handedOver.current) {
        keepPassedCheck(null);
        void engine.abandonOnlineSetup().catch(() => undefined);
      }
    },
    [engine],
  );

  const back =
    from === "online"
      ? { to: "/settings/online", label: t("screen.online.title") }
      : { to: "/settings/advanced", label: t("screen.advanced.title") };

  return (
    <Connect
      form={form}
      setForm={setForm}
      outcome={outcome}
      setOutcome={setOutcome}
      back={back}
      onChecking={() => {
        checked.current = true;
        keepPassedCheck(null);
      }}
      onNext={() => {
        if (outcome?.type !== "works") return;
        keepPassedCheck({ host: outcome.host, model: outcome.model });
        handedOver.current = true;
        goTo("/settings/online/setup", { step: "send" });
      }}
      onLeave={() => goTo(back.to)}
    />
  );
}
