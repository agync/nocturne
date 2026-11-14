import { useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import type { OnlineView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { Consent } from "./Consent";
import { keepPassedCheck, passedCheck } from "./connection";
import type { Draft, Provider, StepProps } from "./draft";
import { Explainer } from "./Explainer";
import { LimitStep } from "./Limit";
import { ServiceStep } from "./ServiceStep";
import { WhatToSend } from "./WhatToSend";

export type Step = "explainer" | "connect" | "send" | "limit" | "consent";
const ORDER: Step[] = ["explainer", "connect", "send", "limit", "consent"];

/** Setting Online answers up, one step at a time. Nothing is turned on until the last button. */
export function SetupFlow({ view, start }: { view: OnlineView; start: Step | null }) {
  const engine = useEngine();
  const [step, setStep] = useState<Step>(start ?? "explainer");
  const [passed] = useState<Provider | null>(passedCheck);
  const [draft, setDraft] = useState<Draft>({
    phones: view.phonesAvailable ? view.phones : false,
    sendSensitive: view.sendSensitive,
    limit: view.limit,
  });
  const checked = useRef(passed !== null);
  const handedOver = useRef(false);
  const first = useRef(true);

  // Leaving once a check has been run, in any way, tells the computer to drop the key and the
  // connection it was given for the check. Turning on keeps them, so it hands over first.
  useEffect(
    () => () => {
      keepPassedCheck(null);
      if (checked.current && !handedOver.current) {
        void engine.abandonOnlineSetup().catch(() => undefined);
      }
    },
    [engine],
  );

  // Each step's heading takes focus, so a keyboard or screen reader person starts at the top.
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const heading = document.querySelector<HTMLElement>("main h1, h1");
    heading?.focus({ preventScroll: true });
  }, [step]);

  const leave = () => goTo("/settings/online");
  const provider = passed;
  const at = provider ? step : step === "explainer" ? "explainer" : "connect";
  const move = (delta: number) => {
    const next = ORDER[Math.min(ORDER.length - 1, Math.max(0, ORDER.indexOf(at) + delta))];
    if (next) setStep(next);
  };

  if (at === "explainer") return <Explainer onNext={() => setStep("connect")} onLeave={leave} />;
  if (at === "connect" || !provider) {
    return (
      <ServiceStep host={provider?.host ?? null} onNext={() => setStep("send")} onLeave={leave} />
    );
  }

  const props: StepProps = {
    draft,
    provider,
    costReported: view.costReported,
    phonesAvailable: view.phonesAvailable,
    setDraft,
    onBack: () => move(-1),
    onNext: () => move(1),
  };
  if (at === "send") return <WhatToSend {...props} />;
  if (at === "limit") return <LimitStep {...props} />;
  return (
    <Consent
      {...props}
      onGo={setStep}
      onLeave={leave}
      onHandOver={() => {
        handedOver.current = true;
      }}
    />
  );
}
