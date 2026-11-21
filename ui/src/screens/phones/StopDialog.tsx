import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { PhoneSource, PhoneSources } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { Confirm } from "../away/parts";
import { PHONE_SOURCES_KEY } from "./usePhoneSources";

interface Props {
  phone: PhoneSources;
  source: PhoneSource;
  onClose: () => void;
  /** Called once the computer has stopped taking it, before the dialog closes. */
  onStopped: () => void;
}

/** "Stop taking the calendar from {phone}?" What already came in stays. */
export function StopDialog({ phone, source, onClose, onStopped }: Props) {
  const engine = useEngine();
  const client = useQueryClient();
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const stop = async () => {
    setBusy(true);
    try {
      client.setQueryData(
        PHONE_SOURCES_KEY,
        await engine.setPhoneSource(phone.deviceId, source, false),
      );
      onStopped();
      onClose();
    } catch {
      toast.show({ message: t("phones.failed") });
      setBusy(false);
    }
  };

  return (
    <Confirm
      title={t(source === "calendar" ? "phones.stop.title.calendar" : "phones.stop.title.health", {
        phone: phone.name,
      })}
      confirm={t("phones.stop.confirm")}
      keep={t("phones.stop.keep")}
      busy={busy}
      onClose={onClose}
      onConfirm={() => void stop()}
    >
      <p>{t("phones.stop.body", { phone: phone.name })}</p>
    </Confirm>
  );
}
