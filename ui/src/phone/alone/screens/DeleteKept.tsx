import { useCallback, useEffect, useId, useRef, useState } from "react";
import { t } from "../../../copy/t";
import type { PhoneDeletePreview, PhoneSource } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import { HomeRequestError } from "../api";
import { Done } from "./Backup";

type Load =
  | { name: "loading" }
  | { name: "ready"; source: PhoneSource; preview: PhoneDeletePreview }
  | { name: "nothing" }
  | { name: "failed" };

/**
 * "Delete what was kept…": the computer's From your phones delete preview, worded for this phone.
 * Opened from This phone it lets the person choose the calendar or Health; from either screen it
 * starts on that one.
 */
export function DeleteKept({
  first,
  choose,
  preview,
  remove,
  onClose,
}: {
  first: PhoneSource;
  /** Whether both can be chosen. */
  choose: boolean;
  preview(source: PhoneSource): Promise<PhoneDeletePreview>;
  remove(token: string, source: PhoneSource, alsoStop: boolean): Promise<void>;
  onClose(deleted: boolean): void;
}) {
  const [source, setSource] = useState<PhoneSource>(first);
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [alsoStop, setAlsoStop] = useState(true);
  const [busy, setBusy] = useState(false);
  const [refreshed, setRefreshed] = useState(false);
  const [failed, setFailed] = useState(false);
  const [done, setDone] = useState(false);
  const latest = useRef(0);
  const stopId = useId();
  const calendar = source === "calendar";
  const heading = useHeading("deleteKept");

  const get = useCallback(async () => {
    const mine = ++latest.current;
    setLoad({ name: "loading" });
    try {
      const got = await preview(source);
      if (mine === latest.current) setLoad({ name: "ready", source, preview: got });
    } catch (e) {
      if (mine !== latest.current) return;
      const none = e instanceof HomeRequestError && e.code === "notFound";
      setLoad(none ? { name: "nothing" } : { name: "failed" });
    }
  }, [preview, source]);

  useEffect(() => {
    void get();
  }, [get]);

  const ready = load.name === "ready" && load.source === source ? load.preview : null;
  const go = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await remove(ready.token, source, alsoStop);
      setDone(true);
    } catch (e) {
      setBusy(false);
      if (e instanceof HomeRequestError && e.code === "tooLate") {
        setRefreshed(true);
        await get();
      } else {
        setFailed(true);
      }
    }
  };

  if (done)
    return (
      <Done
        title={t("phones.delete.done")}
        body={
          calendar
            ? t("phone.alone.deleteKept.untouched.calendar")
            : t("phone.alone.deleteKept.untouched.health")
        }
        button={t("phone.sources.ok")}
        onDone={() => onClose(true)}
      />
    );

  return (
    <Page
      bar={
        <header className="ph-bar ph-bar--back">
          <button
            type="button"
            className="ph-ib ph-ib--ink"
            onClick={() => onClose(false)}
            aria-label={t("phone.cancel")}
          >
            <Icon name="close" />
          </button>
        </header>
      }
    >
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t(
            calendar
              ? "phone.alone.deleteKept.title.calendar"
              : "phone.alone.deleteKept.title.health",
          )}
        </h1>
        <p className="ph-body ph-ink2">
          {calendar ? t("phone.alone.deleteKept.lead") : t("phone.alone.deleteKept.lead.health")}
        </p>
        {choose ? (
          <div className="ph-kept-choice" role="radiogroup" aria-labelledby={`${stopId}-choose`}>
            <p className="ph-lbl" id={`${stopId}-choose`}>
              {t("phones.delete.choose")}
            </p>
            {(["calendar", "health"] as const).map((value) => (
              <label key={value}>
                <input
                  type="radio"
                  name="ph-delete-kept"
                  checked={source === value}
                  disabled={busy}
                  onChange={() => {
                    setRefreshed(false);
                    setAlsoStop(true);
                    setSource(value);
                  }}
                />
                {t(value === "calendar" ? "phone.calendar.label" : "phone.health.label")}
              </label>
            ))}
          </div>
        ) : null}
        {load.name === "failed" ? (
          <p className="ph-error" role="alert">
            {t("phones.delete.error")}
          </p>
        ) : load.name === "nothing" ? (
          <p className="ph-small" role="status">
            {t("forget.nothing")}
          </p>
        ) : ready === null ? (
          <p className="ph-small" role="status">
            {t("phones.delete.loading")}
          </p>
        ) : (
          <>
            {refreshed ? (
              <p className="ph-small" role="status">
                {t("phones.delete.refreshed")}
              </p>
            ) : null}
            <ul className="ph-delete-rows">
              <li>
                {t(
                  calendar
                    ? "phone.alone.deleteKept.rows.calendar"
                    : "phone.alone.deleteKept.rows.health",
                  { count: ready.items },
                )}
              </li>
              {ready.pageLines > 0 ? (
                <li>{t("phones.delete.lines.calendar", { count: ready.pageLines })}</li>
              ) : null}
              {ready.recapLines > 0 ? (
                <li>{t("phones.delete.lines.health", { count: ready.recapLines })}</li>
              ) : null}
              {ready.noticing > 0 ? (
                <li>{t("phones.delete.noticing", { count: ready.noticing })}</li>
              ) : null}
              {(ready.sharedMemories ?? 0) > 0 ? (
                <li>{t("phones.delete.shared", { count: ready.sharedMemories ?? 0 })}</li>
              ) : null}
            </ul>
          </>
        )}
        {ready !== null ? (
          <label className="ph-check" htmlFor={stopId}>
            <input
              id={stopId}
              type="checkbox"
              checked={alsoStop}
              disabled={busy}
              onChange={(e) => setAlsoStop(e.target.checked)}
            />
            <span className="ph-srow-text">
              <span>
                {calendar
                  ? t("phone.alone.deleteKept.alsoStop")
                  : t("phone.alone.deleteKept.alsoStop.health")}
              </span>
              <span className="ph-srow-hint">
                {calendar
                  ? t("phone.alone.deleteKept.alsoStop.hint")
                  : t("phone.alone.deleteKept.alsoStop.health.hint")}
              </span>
            </span>
          </label>
        ) : null}
        <p className="ph-small">
          {calendar
            ? t("phone.alone.deleteKept.untouched.calendar")
            : t("phone.alone.deleteKept.untouched.health")}
        </p>
        {failed ? (
          <p className="ph-error" role="alert">
            {t("phones.failed")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button onClick={() => onClose(false)}>{t("phones.delete.keep")}</Button>
        <button
          type="button"
          className="ph-btn ph-btn--ember"
          aria-disabled={!ready || busy ? true : undefined}
          onClick={() => void go()}
        >
          {t("phones.delete.confirm")}
        </button>
      </footer>
    </Page>
  );
}
