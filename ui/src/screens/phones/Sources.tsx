import { useEffect, useRef, useState } from "react";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Icon } from "../../components/Icon/Icon";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { Toggle } from "../../components/Toggle/Toggle";
import { t } from "../../copy/t";
import type { PhoneSource, PhoneSources, SourceState } from "../../engine/bindings";
import { DeleteDialog } from "./DeleteDialog";
import styles from "./Phones.module.css";
import { StopDialog } from "./StopDialog";
import { cameInWhen, kindsStarting, phoneKind, usePhoneSources } from "./usePhoneSources";

interface RowProps {
  phone: PhoneSources;
  source: PhoneSource;
  state: SourceState;
  onStop: () => void;
  onAgain: () => void;
}

function SourceRow({ phone, source, state, onStop, onAgain }: RowProps) {
  const calendar = source === "calendar";
  const last = state.lastInAt ? cameInWhen(state.lastInAt, "sentence") : null;
  const sourceName = t(calendar ? "phones.source.calendar" : "phones.source.health");
  let summary: string;
  if (calendar) {
    summary = last
      ? t("phones.calendar.summary", { count: state.calendars, time: last })
      : t("phones.calendar.count", { count: state.calendars });
  } else {
    const kinds = kindsStarting(state.kinds);
    summary = last ? t("phones.health.summary", { kinds, time: last }) : kinds;
  }
  return (
    <li className={styles.source} data-source={source}>
      <div className={styles.sourceText}>
        <span className={styles.sourceLabel}>
          <Icon name={calendar ? "calendar" : "heart"} size={18} />
          {t(calendar ? "phones.calendar.label" : "phones.health.label")}
        </span>
        <span className={styles.muted}>{summary}</span>
        {calendar && state.taking ? (
          <span className={styles.muted}>
            {t("phones.calendar.upcoming", { count: state.upcoming })}
          </span>
        ) : null}
      </div>
      <div className={styles.actions}>
        {state.taking ? (
          <>
            <a
              className={styles.linkButton}
              href={
                calendar
                  ? `#/settings/phones/came-in/${phone.deviceId}/calendar`
                  : "#/journal/health"
              }
              aria-label={t(
                calendar ? "phones.calendar.seeCameIn.named" : "phones.health.seeReadings.named",
                { phone: phone.name },
              )}
            >
              {t(calendar ? "phones.calendar.seeCameIn" : "phones.health.seeReadings")}
            </a>
            <Button
              variant="quiet"
              aria-label={t("phones.stop.named", { source: sourceName, phone: phone.name })}
              onClick={onStop}
            >
              {t("phones.stop")}
            </Button>
          </>
        ) : (
          <>
            <span className={styles.stopped}>{t("phones.stopped")}</span>
            <Button
              variant="quiet"
              aria-label={t("phones.again.named", { source: sourceName, phone: phone.name })}
              onClick={onAgain}
            >
              {t("phones.again")}
            </Button>
          </>
        )}
      </div>
    </li>
  );
}

function PhoneBlock({ phone }: { phone: PhoneSources }) {
  const sources = usePhoneSources();
  const toast = useToast();
  const [stopping, setStopping] = useState<PhoneSource | null>(null);
  const [deleting, setDeleting] = useState(false);
  const refocus = useRef<PhoneSource | null>(null);
  const block = useRef<HTMLLIElement>(null);
  const kind = phoneKind(phone);
  const brings = (["calendar", "health"] as const).filter((s) => phone[s] !== null);
  const hasHealth = phone.health !== null;
  const view = sources.data;

  // After the row changes, the button that opened the dialog is gone; keep the keyboard in the row.
  useEffect(() => {
    const source = refocus.current;
    if (!source) return;
    refocus.current = null;
    block.current?.querySelector<HTMLElement>(`[data-source="${source}"] button`)?.focus();
  });

  const again = async (source: PhoneSource) => {
    try {
      refocus.current = source;
      await sources.setSource(phone.deviceId, source, true);
    } catch {
      refocus.current = null;
      toast.show({ message: t("phones.failed") });
    }
  };

  const noticing = async (on: boolean) => {
    try {
      await sources.setNoticing(on);
    } catch {
      toast.show({ message: t("phones.failed") });
    }
  };

  return (
    <li ref={block} className={styles.phone}>
      <div className={styles.phoneHead}>
        <h2 className={styles.phoneName}>{phone.name}</h2>
        {kind ? <span className={styles.kind}>{kind}</span> : null}
      </div>
      {phone.inBrowser ? (
        <p className={styles.muted}>{t("phones.browser.note")}</p>
      ) : (
        <ul className={styles.rows}>
          {brings.map((source) => {
            const state = phone[source];
            return state ? (
              <SourceRow
                key={source}
                phone={phone}
                source={source}
                state={state}
                onStop={() => setStopping(source)}
                onAgain={() => void again(source)}
              />
            ) : null;
          })}
          {hasHealth && view ? (
            <li>
              <Toggle
                checked={view.noticing}
                onChange={(on) => void noticing(on)}
                label={t("phones.noticing.label")}
                hint={t("phones.noticing.hint")}
              />
            </li>
          ) : null}
          {brings.length > 0 ? (
            <li className={styles.deleteLink}>
              <Button
                variant="forget"
                flush
                aria-label={t("phones.delete.link.named", { phone: phone.name })}
                onClick={() => setDeleting(true)}
              >
                {t("phones.delete.link")}
              </Button>
              <span className={styles.muted}>{t("phones.delete.hint")}</span>
            </li>
          ) : null}
        </ul>
      )}
      {stopping ? (
        <StopDialog
          phone={phone}
          source={stopping}
          onClose={() => setStopping(null)}
          onStopped={() => {
            refocus.current = stopping;
          }}
        />
      ) : null}
      {deleting ? (
        <DeleteDialog
          phone={phone}
          source={phone.calendar ? "calendar" : "health"}
          onClose={() => setDeleting(false)}
        />
      ) : null}
    </li>
  );
}

/** #/settings/phones: what each phone brings, and the few choices that go with it. */
export function Sources() {
  const sources = usePhoneSources();
  const toast = useToast();
  const { data, phones } = sources;
  const back = <BackLink to="/settings" label={t("screen.settings.title")} />;

  if (sources.isError) {
    return (
      <ScreenSheet>
        {back}
        <div className={styles.message} role="alert">
          <p>{t("phones.error")}</p>
          <Button variant="secondary" onClick={() => void sources.refetch()}>
            {t("button.tryAgain")}
          </Button>
        </div>
      </ScreenSheet>
    );
  }
  if (!data || !phones) {
    return (
      <ScreenSheet>
        {back}
        <p className={styles.muted} role="status">
          {t("phones.loading")}
        </p>
      </ScreenSheet>
    );
  }

  const change = async (next: Partial<typeof data.page>) => {
    try {
      await sources.setPageOptions({ ...data.page, ...next });
    } catch {
      toast.show({ message: t("phones.failed") });
    }
  };
  const anyApp = phones.some((p) => !p.inBrowser);

  return (
    <ScreenSheet>
      {back}
      <div className={styles.main}>
        <h1 className="t-page-title">{t("screen.phones.title")}</h1>
        <p className={styles.lede}>{t("phones.lead")}</p>
        {phones.length === 0 ? (
          <p className={styles.lede}>{t("phones.empty")}</p>
        ) : (
          <ul className={styles.phones} aria-label={t("phones.list")}>
            {phones.map((phone) => (
              <PhoneBlock key={phone.deviceId} phone={phone} />
            ))}
          </ul>
        )}
        {anyApp ? (
          <section className={styles.group} aria-labelledby="phones-page">
            <h2 id="phones-page" className={styles.groupLabel}>
              {t("phones.page.heading")}
            </h2>
            <ul className={styles.rows}>
              <li>
                <Toggle
                  checked={data.page.calendarToday}
                  onChange={(on) => void change({ calendarToday: on })}
                  label={t("phones.page.calendar.label")}
                  hint={t("phones.page.calendar.hint")}
                />
              </li>
              <li>
                <Toggle
                  checked={data.page.sleepLastNight}
                  onChange={(on) => void change({ sleepLastNight: on })}
                  label={t("phones.page.sleep.label")}
                  hint={t("phones.page.sleep.hint")}
                />
              </li>
            </ul>
            <p className={styles.foot}>{t("phones.foot")}</p>
          </section>
        ) : null}
      </div>
    </ScreenSheet>
  );
}
