import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Checkbox } from "../../components/Checkbox/Checkbox";
import { Dialog } from "../../components/Dialog/Dialog";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { PhoneDeletePreview, PhoneSource, PhoneSources } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { useEngine, useRefresh } from "../../engine/context";
import { DeleteRows } from "./DeleteRows";
import styles from "./Phones.module.css";
import { PHONE_SOURCES_KEY } from "./usePhoneSources";

interface Props {
  phone: PhoneSources;
  /** The source to start on. A phone that brings both lets the person choose. */
  source: PhoneSource;
  onClose: () => void;
}

/** What deleting one source of a phone would remove, and the one question that goes with it. */
export function DeleteDialog({ phone, source: first, onClose }: Props) {
  const engine = useEngine();
  const client = useQueryClient();
  const refresh = useRefresh();
  const toast = useToast();
  const [source, setSource] = useState<PhoneSource>(first);
  const [loaded, setLoaded] = useState<{ source: PhoneSource; preview: PhoneDeletePreview } | null>(
    null,
  );
  const latest = useRef(0);
  const [failed, setFailed] = useState(false);
  const [refreshed, setRefreshed] = useState(false);
  const [alsoStop, setAlsoStop] = useState(true);
  const [busy, setBusy] = useState(false);
  const buttons = useRef<HTMLDivElement>(null);
  const both = phone.calendar !== null && phone.health !== null;
  const kinds = phone.health?.kinds ?? [];

  const load = useCallback(async () => {
    const request = ++latest.current;
    setLoaded(null);
    setFailed(false);
    try {
      const preview = await engine.previewPhoneDelete({
        deviceId: phone.deviceId,
        source,
        kinds: null,
        before: null,
      });
      if (request === latest.current) setLoaded({ source, preview });
    } catch {
      if (request === latest.current) setFailed(true);
    }
  }, [engine, phone.deviceId, source]);

  useEffect(() => {
    void load();
  }, [load]);

  // The first answer is the one that changes nothing.
  useEffect(() => {
    buttons.current?.querySelector("button")?.focus();
  }, []);

  const preview = loaded && loaded.source === source ? loaded : null;
  const remove = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      await engine.deletePhoneData(preview.preview.token, source === "calendar" && alsoStop);
      void client.invalidateQueries({ queryKey: PHONE_SOURCES_KEY });
      refresh("page", "journal", "items", "status");
      toast.show({ message: t("phones.delete.done") });
      onClose();
    } catch (error) {
      if (error instanceof EngineError && error.code === "tokenExpired") {
        setRefreshed(true);
        await load();
      } else {
        toast.show({ message: t("phones.failed") });
      }
      setBusy(false);
    }
  };

  const calendar = source === "calendar";
  return (
    <Dialog
      title={t(calendar ? "phones.delete.title.calendar" : "phones.delete.title.health", {
        phone: phone.name,
      })}
      onClose={onClose}
      locked={busy}
    >
      <div className={styles.dialog}>
        <p className={styles.muted}>{t("phones.delete.lead")}</p>
        {both ? (
          <fieldset className={styles.choice}>
            <legend className={styles.choiceLegend}>{t("phones.delete.choose")}</legend>
            {(["calendar", "health"] as const).map((value) => (
              <label key={value} className={styles.choiceOption}>
                <input
                  type="radio"
                  name="phone-delete-source"
                  checked={source === value}
                  disabled={busy}
                  onChange={() => setSource(value)}
                />
                {t(value === "calendar" ? "phones.calendar.label" : "phones.health.label")}
              </label>
            ))}
          </fieldset>
        ) : null}
        {failed ? (
          <p role="alert">{t("phones.delete.error")}</p>
        ) : preview === null ? (
          <p role="status" className={styles.muted}>
            {t("phones.delete.loading")}
          </p>
        ) : (
          <>
            {refreshed ? (
              <p role="status" className={styles.muted}>
                {t("phones.delete.refreshed")}
              </p>
            ) : null}
            <DeleteRows
              source={source}
              preview={preview.preview}
              phoneName={phone.name}
              kinds={kinds}
            />
          </>
        )}
        {calendar ? (
          <Checkbox
            checked={alsoStop}
            onChange={setAlsoStop}
            disabled={busy}
            label={t("phones.delete.alsoStop", { phone: phone.name })}
            hint={t("phones.delete.alsoStop.hint", { phone: phone.name })}
          />
        ) : null}
        <p className={styles.muted}>
          {t(calendar ? "phones.delete.untouched.calendar" : "phones.delete.untouched.health")}{" "}
          {t("forget.stays.out")}
        </p>
        <div className={styles.buttons} ref={buttons}>
          <Button variant="secondary" onClick={onClose} disabled={busy}>
            {t("phones.delete.keep")}
          </Button>
          <Button
            variant="forget"
            confirming
            disabled={busy || preview === null}
            onClick={() => void remove()}
          >
            {t("phones.delete.confirm")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
