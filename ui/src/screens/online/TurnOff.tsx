import { useCallback } from "react";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { OnlineView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Online.module.css";
import { Confirm } from "./parts";
import { useOnlineAct } from "./useOnline";

/** The settings Online answers had, so turning it back on needs nothing more. */
export const settingsOf = (view: OnlineView) => ({
  phones: view.phones,
  sendSensitive: view.sendSensitive,
  limit: view.limit,
});

/** Turns Online answers off at once, with no question; Undo turns it back on. */
export function useTurnOff(view: OnlineView) {
  const engine = useEngine();
  const toast = useToast();
  const { run, busy } = useOnlineAct();
  const turnOff = useCallback(async () => {
    const before = settingsOf(view);
    const done = await run(() => engine.turnOffOnline(false));
    if (!done) return;
    toast.show({
      message: t("online.toast.off"),
      action: {
        label: t("online.toast.undo"),
        onAction: () => void run(() => engine.turnOnOnline(before)),
      },
    });
  }, [engine, run, toast, view]);
  return { turnOff, busy };
}

/** Forgetting the key says what it removes, names the service and what stays listed. */
export function ForgetKeyDialog({ view, onClose }: { view: OnlineView; onClose: () => void }) {
  const engine = useEngine();
  const toast = useToast();
  const { run, busy } = useOnlineAct();
  const off = view.state.type === "off";
  const host = view.host ?? "";
  return (
    <Confirm
      title={t(off ? "online.forget.titleOff" : "online.forget.title")}
      confirm={t(off ? "online.forget.confirmOff" : "online.forget.confirm")}
      keep={t(off ? "online.forget.keepOff" : "online.forget.keep")}
      destructive
      busy={busy}
      onClose={onClose}
      onConfirm={() => {
        void (async () => {
          const done = await run(() => engine.turnOffOnline(true));
          onClose();
          if (done)
            toast.show({ message: t(off ? "online.toast.forgotOff" : "online.toast.forgot") });
        })();
      }}
    >
      <p className={styles.strong}>
        {t(off ? "online.forget.bodyOff" : "online.forget.body", { host })}
      </p>
      <p>{t("online.forget.stillWorks")}</p>
      <p>{t("online.forget.history")}</p>
    </Confirm>
  );
}
