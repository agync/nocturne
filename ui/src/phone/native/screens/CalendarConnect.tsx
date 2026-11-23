import { useEffect, useState } from "react";
import { t } from "../../../copy/t";
import type { PhoneCalendar } from "../../../engine/bindings";
import { BackBar, Bar, Button, Page } from "../../screens/ui";
import type { CalendarSettings, NativeChannel, ThisPhoneSources } from "../api";
import { askCalendar, type CalendarAccess, calendarAccess, openAppSettings } from "../sources";
import {
  DoneBar,
  KeptSoFar,
  openPolicy,
  places,
  placesRich,
  Switch,
  sentAt,
  useHeading,
} from "./parts";

type Step = "loading" | "why" | "refused" | "settings";

/**
 * Bringing the calendar: Nocturne's own screen first, then the phone's prompt, then "Which
 * calendars" when allowed or "stays on this phone" when not. Once brought, the same screen is
 * the calendar's settings.
 */
export function CalendarConnect({
  channel,
  onDone,
  alone = false,
  onDeleteKept,
}: {
  channel: NativeChannel;
  onDone(): void;
  /** A phone alone: the calendar stays in this phone's own memory. */
  alone?: boolean;
  /** A phone alone: "Delete what was kept…" under the line about what was already kept. */
  onDeleteKept?(): void;
}) {
  const [step, setStep] = useState<Step>("loading");
  const [access, setAccess] = useState<CalendarAccess>("ask");

  useEffect(() => {
    let gone = false;
    void Promise.all([calendarAccess(), channel.calendarSettings()]).then(([a, s]) => {
      if (gone) return;
      setAccess(a);
      setStep(s.bring && a === "granted" ? "settings" : "why");
    });
    return () => {
      gone = true;
    };
  }, [channel]);

  // Back from the phone's settings: the way on may have changed.
  useEffect(() => {
    const seen = () => {
      if (document.visibilityState === "visible") void calendarAccess().then(setAccess);
    };
    document.addEventListener("visibilitychange", seen);
    return () => document.removeEventListener("visibilitychange", seen);
  }, []);

  const ask = async () => {
    const answer = await askCalendar();
    setAccess(answer);
    if (answer !== "granted") return setStep("refused");
    const settings = await channel.calendarSettings();
    await channel.setCalendarSettings({ ...settings, bring: true });
    setStep("settings");
  };

  switch (step) {
    case "loading":
      return null;
    case "why":
      return (
        <CalendarWhy
          alone={alone}
          blocked={access === "blocked"}
          onContinue={() => void ask()}
          onSettings={() => void openAppSettings()}
          onNotNow={() => setStep("refused")}
        />
      );
    case "refused":
      return <CalendarRefused onOk={onDone} />;
    case "settings":
      return (
        <CalendarSettingsScreen
          channel={channel}
          onBack={onDone}
          alone={alone}
          onDeleteKept={onDeleteKept}
        />
      );
  }
}

export function CalendarWhy({
  alone = false,
  blocked,
  onContinue,
  onSettings,
  onNotNow,
}: {
  alone?: boolean;
  /** The phone will no longer ask: the way on is its settings. */
  blocked: boolean;
  onContinue(): void;
  onSettings(): void;
  onNotNow(): void;
}) {
  const heading = useHeading("calendarWhy");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <span className="ph-lbl">{t("phone.calendar.label")}</span>
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {alone ? t("phone.alone.calendar.why.title") : t("phone.calendar.why.title")}
        </h1>
        <p className="ph-body">{t("phone.calendar.why.what")}</p>
        <p className="ph-body">
          {alone ? t("phone.alone.calendar.why.where") : t("phone.calendar.why.where")}
        </p>
        <p className="ph-body">
          {alone ? t("phone.alone.calendar.why.for") : t("phone.calendar.why.for")}
        </p>
        <p className="ph-body">{t("phone.calendar.why.never")}</p>
        {blocked ? (
          <p className="ph-hint">{t("phone.calendar.off.title")}</p>
        ) : (
          <p className="ph-hint">{t("phone.calendar.why.next")}</p>
        )}
        <div className="ph-spacer" />
        <div className="ph-stack">
          {blocked ? (
            <Button kind="primary" onClick={onSettings}>
              {t("phone.sources.openPhoneSettings")}
            </Button>
          ) : (
            <Button kind="primary" onClick={onContinue}>
              {t("phone.sources.continue")}
            </Button>
          )}
          <Button kind="quiet" onClick={onNotNow}>
            {t("phone.sources.notNow")}
          </Button>
        </div>
        <button type="button" className="ph-native-link" onClick={openPolicy}>
          {t("phone.sources.privacy")}
        </button>
      </main>
    </Page>
  );
}

export function CalendarRefused({ onOk }: { onOk(): void }) {
  const heading = useHeading("calendarRefused");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {t("phone.calendar.refused.title")}
        </h1>
        <p className="ph-body">{placesRich("phone.calendar.refused.body")}</p>
        <div className="ph-spacer" />
        <Button kind="primary" onClick={onOk}>
          {t("phone.sources.ok")}
        </Button>
      </main>
    </Page>
  );
}

/** "Your calendar": the switch, which calendars, notes, and what went last. */
export function CalendarSettingsScreen({
  channel,
  onBack,
  alone = false,
  onDeleteKept,
}: {
  channel: NativeChannel;
  onBack(): void;
  /** A phone alone sends nothing anywhere: no lines about what went across. */
  alone?: boolean;
  onDeleteKept?(): void;
}) {
  const [calendars, setCalendars] = useState<PhoneCalendar[]>([]);
  const [settings, setSettings] = useState<CalendarSettings | null>(null);
  const [status, setStatus] = useState<ThisPhoneSources | null>(null);
  const [stopped, setStopped] = useState(false);

  useEffect(() => {
    let gone = false;
    void Promise.all([channel.calendars(), channel.calendarSettings(), channel.sources()]).then(
      ([c, s, st]) => {
        if (gone) return;
        setCalendars(c);
        setSettings(s);
        setStatus(st);
      },
    );
    return () => {
      gone = true;
    };
  }, [channel]);

  if (!settings) return null;

  const save = async (next: CalendarSettings) => {
    setSettings(next);
    setStopped(!next.bring);
    setStatus(await channel.setCalendarSettings(next));
  };

  const lastSent = status?.calendarLastSent;
  const names = calendars.map((c) => c.name);
  const shared = new Set(names.filter((n, i) => names.indexOf(n) !== i));
  return (
    <Page bar={<BackBar title={t("phone.calendar.label")} onBack={onBack} />}>
      <main className="ph-main ph-bring">
        {!alone && status?.calendar === "stoppedOnComputer" && (
          <div className="ph-card" role="status">
            <p className="ph-card-title">{t("phone.calendar.stoppedOnComputer.title")}</p>
            <p className="ph-hint">{placesRich("phone.calendar.stoppedOnComputer.body")}</p>
          </div>
        )}
        {status?.calendar === "offInSettings" && (
          <div className="ph-card" role="status">
            <p className="ph-card-title">{t("phone.calendar.off.title")}</p>
            <p className="ph-hint">{t("phone.calendar.off.body")}</p>
            <Button kind="quiet" onClick={() => void openAppSettings()}>
              {t("phone.sources.openPhoneSettings")}
            </Button>
          </div>
        )}
        <Switch
          label={alone ? t("phone.alone.calendar.bring") : t("phone.calendar.bring")}
          hint={t("phone.calendar.bring.hint")}
          on={settings.bring}
          onChange={(bring) => void save({ ...settings, bring })}
        />
        {stopped && !alone && (
          <p className="ph-hint" role="status">
            {t("phone.calendar.stopped")}
          </p>
        )}
        <h2 className="ph-lbl">{t("phone.calendar.which")}</h2>
        <div className="ph-bring-list">
          {calendars.map((c) => (
            <Switch
              key={c.id}
              label={c.name}
              sub={shared.has(c.name) ? c.account : undefined}
              dot={c.colour}
              on={settings.calendars[c.id] ?? true}
              onChange={(on) =>
                void save({ ...settings, calendars: { ...settings.calendars, [c.id]: on } })
              }
            />
          ))}
        </div>
        <p className="ph-hint">
          {alone ? t("phone.alone.calendar.note") : t("phone.calendar.note")}
        </p>
        {status?.calendarTooMany && (
          <p className="ph-hint" role="status">
            {t("phone.calendar.tooMany")}
          </p>
        )}
        <Switch
          label={t("phone.calendar.notes")}
          hint={t("phone.calendar.notes.hint")}
          on={settings.notes}
          onChange={(notes) => void save({ ...settings, notes })}
        />
        {lastSent && !alone && (
          <p className="ph-hint">
            {t("phone.calendar.lastSent", {
              time: sentAt(lastSent.at),
              count: lastSent.count,
            })}
          </p>
        )}
        {alone ? (
          <KeptSoFar onDelete={onDeleteKept} />
        ) : (
          <p className="ph-hint">{places("phone.sources.already")}</p>
        )}
      </main>
      <DoneBar onDone={onBack} />
    </Page>
  );
}
