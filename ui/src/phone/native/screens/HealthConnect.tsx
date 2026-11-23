import { useEffect, useState } from "react";
import { type CopyId, t } from "../../../copy/t";
import type { ReadingKind } from "../../../engine/bindings";
import { BackBar, Bar, Button, Page } from "../../screens/ui";
import type { HealthSettings, NativeChannel, ThisPhoneSources } from "../api";
import {
  askHealth,
  DEFAULT_KINDS,
  healthAvailability,
  healthOlderAvailable,
  KINDS,
  onIphone,
  openHealth,
} from "../sources";
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

type Step =
  | { name: "loading" }
  | { name: "why" }
  | { name: "kinds" }
  | { name: "done"; kinds: ReadingKind[] }
  | { name: "doneIos" }
  | { name: "none" }
  | { name: "missing" }
  | { name: "settings" };

/** Each kind's name and its reason in one line. */
const KIND_COPY: Record<ReadingKind, [CopyId, CopyId]> = {
  sleep: ["health.kind.sleep", "phone.health.kind.sleep.hint"],
  heartRate: ["health.kind.heartRate", "phone.health.kind.heartRate.hint"],
  restingHeartRate: ["health.kind.restingHeartRate", "phone.health.kind.restingHeartRate.hint"],
  bloodPressure: ["health.kind.bloodPressure", "phone.health.kind.bloodPressure.hint"],
  steps: ["health.kind.steps", "phone.health.kind.steps.hint"],
  workout: ["health.kind.workout", "phone.health.kind.workout.hint"],
  weight: ["health.kind.weight", "phone.health.kind.weight.hint"],
};

const kindName = (k: ReadingKind) => t(KIND_COPY[k][0]);
const kindHint = (k: ReadingKind) => t(KIND_COPY[k][1]);

/** "sleep, heart rate and steps". */
export function kindList(kinds: ReadingKind[]): string {
  const names = KINDS.filter((k) => kinds.includes(k)).map((k) => kindName(k).toLowerCase());
  return new Intl.ListFormat("en-GB", { style: "long", type: "conjunction" }).format(names);
}

/**
 * Bringing Health: Nocturne's own screen, then which kinds, then the phone's own screen to
 * confirm (Health Connect's, or Apple Health's sheet), then what it allowed. An iPhone never says
 * what was allowed, so it answers "Done." Once brought, the same screen is Health's settings.
 */
export function HealthConnect({
  channel,
  onDone,
  alone = false,
  onDeleteKept,
}: {
  channel: NativeChannel;
  onDone(): void;
  /** A phone alone: readings stay in this phone's own memory. */
  alone?: boolean;
  /** A phone alone: "Delete what was kept…" under the line about what was already kept. */
  onDeleteKept?(): void;
}) {
  const [step, setStep] = useState<Step>({ name: "loading" });
  const [kinds, setKinds] = useState<ReadingKind[]>(DEFAULT_KINDS);
  const [ios, setIos] = useState(false);

  useEffect(() => {
    let gone = false;
    void Promise.all([
      healthAvailability(),
      channel.healthSettings(),
      channel.sources(),
      onIphone(),
    ]).then(([available, settings, sources, iphone]) => {
      if (gone) return;
      setKinds(settings.kinds);
      setIos(iphone);
      // Health Connect is the one that can be missing; without Health, an iPhone keeps it.
      if (available !== "available")
        return setStep(iphone ? { name: "none" } : { name: "missing" });
      setStep(
        settings.bring && sources.health !== "notBrought" ? { name: "settings" } : { name: "why" },
      );
    });
    return () => {
      gone = true;
    };
  }, [channel]);

  const confirm = async () => {
    const allowed = await askHealth(kinds, false);
    const brought = kinds.filter((k) => allowed.kinds.includes(k));
    if (brought.length === 0 && !ios) return setStep({ name: "none" });
    const settings = await channel.healthSettings();
    await channel.setHealthSettings({ ...settings, bring: true, kinds });
    setStep(ios ? { name: "doneIos" } : { name: "done", kinds: brought });
  };

  switch (step.name) {
    case "loading":
      return null;
    case "why":
      return (
        <HealthWhy
          ios={ios}
          alone={alone}
          onContinue={() => setStep({ name: "kinds" })}
          onNotNow={() => setStep({ name: "none" })}
        />
      );
    case "kinds":
      return <HealthKinds kinds={kinds} onChange={setKinds} onContinue={() => void confirm()} />;
    case "done":
      return (
        <HealthAnswer
          title={t("phone.health.done.title", { kinds: kindList(step.kinds) })}
          body={alone ? t("phone.alone.health.done.body") : t("phone.health.done.body")}
          onOk={onDone}
        />
      );
    case "doneIos":
      return (
        <HealthAnswer
          title={t("phone.health.doneIos.title")}
          body={alone ? t("phone.alone.health.doneIos.body") : t("phone.health.doneIos.body")}
          onOk={onDone}
        />
      );
    case "none":
      return (
        <HealthAnswer
          title={t("phone.health.none.title")}
          body={places("phone.health.none.body")}
          onOk={onDone}
        />
      );
    case "missing":
      return <HealthMissing onGet={() => void openHealth("healthStore")} onBack={onDone} />;
    case "settings":
      return (
        <HealthSettingsScreen
          channel={channel}
          ios={ios}
          onBack={onDone}
          alone={alone}
          onDeleteKept={onDeleteKept}
        />
      );
  }
}

export function HealthWhy({
  ios = false,
  alone = false,
  onContinue,
  onNotNow,
}: {
  ios?: boolean;
  alone?: boolean;
  onContinue(): void;
  onNotNow(): void;
}) {
  const heading = useHeading("healthWhy");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <span className="ph-lbl">{t("phone.health.label")}</span>
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {alone ? t("phone.alone.health.why.title") : t("phone.health.why.title")}
        </h1>
        <p className="ph-body">
          {t("phone.health.why.what", {
            source: t(ios ? "phone.health.source.ios" : "phone.health.source.android"),
          })}
        </p>
        <p className="ph-body">{t("phone.health.why.far")}</p>
        <p className="ph-body">
          {alone ? t("phone.alone.health.why.where") : t("phone.health.why.where")}
        </p>
        <p className="ph-body">
          {alone ? t("phone.alone.health.why.for") : t("phone.health.why.for")}
        </p>
        <p className="ph-body">{t("phone.health.why.never")}</p>
        <p className="ph-hint">{t("phone.health.why.next")}</p>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onContinue}>
            {t("phone.sources.continue")}
          </Button>
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

function KindSwitches({
  kinds,
  onChange,
  disabled,
}: {
  kinds: ReadingKind[];
  onChange(kinds: ReadingKind[]): void;
  disabled?: boolean;
}) {
  return (
    <div className="ph-bring-list">
      {KINDS.map((k) => (
        <Switch
          key={k}
          label={kindName(k)}
          hint={kindHint(k)}
          on={kinds.includes(k)}
          disabled={disabled}
          onChange={(on) =>
            onChange(
              on ? KINDS.filter((x) => x === k || kinds.includes(x)) : kinds.filter((x) => x !== k),
            )
          }
        />
      ))}
    </div>
  );
}

export function HealthKinds({
  kinds,
  onChange,
  onContinue,
}: {
  kinds: ReadingKind[];
  onChange(kinds: ReadingKind[]): void;
  onContinue(): void;
}) {
  const heading = useHeading("healthKinds");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <span className="ph-lbl">{t("phone.health.label")}</span>
        <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
          {t("phone.health.kinds.title")}
        </h1>
        <KindSwitches kinds={kinds} onChange={onChange} />
        <div className="ph-spacer" />
        <Button kind="primary" onClick={onContinue} disabled={kinds.length === 0}>
          {t("phone.sources.continue")}
        </Button>
        <p className="ph-hint">{placesRich("phone.health.kinds.foot")}</p>
      </main>
    </Page>
  );
}

function HealthAnswer({ title, body, onOk }: { title: string; body: string; onOk(): void }) {
  const heading = useHeading(title);
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {title}
        </h1>
        <p className="ph-body">{body}</p>
        <div className="ph-spacer" />
        <Button kind="primary" onClick={onOk}>
          {t("phone.sources.ok")}
        </Button>
      </main>
    </Page>
  );
}

export function HealthMissing({ onGet, onBack }: { onGet(): void; onBack(): void }) {
  const heading = useHeading("healthMissing");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {t("phone.health.missing.title")}
        </h1>
        <p className="ph-body">{t("phone.health.missing.body")}</p>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onGet}>
            {t("phone.health.missing.get")}
          </Button>
          <Button kind="quiet" onClick={onBack}>
            {t("phone.sources.notNow")}
          </Button>
        </div>
      </main>
    </Page>
  );
}

/** "21 October", or "21 November 2025" a year back; before the first read, what it will be. */
export function fromDay(day: string | null, older: boolean, now = new Date()): string {
  let d = day ? new Date(`${day}T12:00:00`) : new Date(now);
  if (!day) d.setDate(d.getDate() - (older ? 364 : 29));
  if (Number.isNaN(d.getTime())) d = new Date(now);
  const yearAgo = d.getFullYear() !== now.getFullYear() || older;
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "long",
    ...(yearAgo ? { year: "numeric" } : {}),
  }).format(d);
}

/** Health on This phone: the switch, the kinds, how far back, and what went last. */
export function HealthSettingsScreen({
  channel,
  ios = false,
  onBack,
  alone = false,
  onDeleteKept,
}: {
  channel: NativeChannel;
  ios?: boolean;
  onBack(): void;
  /** A phone alone sends nothing anywhere: no lines about what went across. */
  alone?: boolean;
  onDeleteKept?(): void;
}) {
  const [settings, setSettings] = useState<HealthSettings | null>(null);
  const [status, setStatus] = useState<ThisPhoneSources | null>(null);
  const [olderOffered, setOlderOffered] = useState(false);

  useEffect(() => {
    let gone = false;
    void Promise.all([channel.healthSettings(), channel.sources(), healthOlderAvailable()]).then(
      ([s, st, older]) => {
        if (gone) return;
        setSettings(s);
        setStatus(st);
        setOlderOffered(older);
      },
    );
    return () => {
      gone = true;
    };
  }, [channel]);

  if (!settings) return null;

  const save = async (next: HealthSettings) => {
    setSettings(next);
    setStatus(await channel.setHealthSettings(next));
  };

  // A kind turned on, or older readings, needs the phone's own yes first.
  const ask = async (next: HealthSettings) => {
    const allowed = await askHealth(next.kinds, next.olderReadings);
    await save({ ...next, olderReadings: next.olderReadings && allowed.history });
  };

  const lastSent = status?.healthLastSent;
  // An iPhone never says a kind was turned off: nothing at all after the first read says it.
  const quiet = ios && settings.bring && status?.healthFrom != null && !lastSent?.count;
  return (
    <Page bar={<BackBar title={t("phone.health.label")} onBack={onBack} />}>
      <main className="ph-main ph-bring">
        {quiet && (
          <div className="ph-card" role="status">
            <p className="ph-hint">{t("phone.health.quietIos")}</p>
          </div>
        )}
        {!ios && status?.health === "offInSettings" && (
          <div className="ph-card" role="status">
            <p className="ph-card-title">{t("phone.health.takenBack.title")}</p>
            <p className="ph-hint">{t("phone.health.takenBack.body")}</p>
            <Button kind="quiet" onClick={() => void openHealth("health")}>
              {t("phone.health.takenBack.open")}
            </Button>
          </div>
        )}
        <Switch
          label={alone ? t("phone.alone.health.bring") : t("phone.health.bring")}
          on={settings.bring}
          onChange={(bring) => void save({ ...settings, bring })}
        />
        <h2 className="ph-lbl">{t("phone.health.kinds.title")}</h2>
        <KindSwitches
          kinds={settings.kinds}
          disabled={!settings.bring}
          onChange={(kinds) => {
            const added = kinds.some((k) => !settings.kinds.includes(k));
            void (added ? ask : save)({ ...settings, kinds });
          }}
        />
        <p className="ph-hint">
          {t("phone.health.from", {
            date: fromDay(status?.healthFrom ?? null, settings.olderReadings),
          })}
        </p>
        {olderOffered && (
          <Switch
            label={t("phone.health.older")}
            hint={t("phone.health.older.hint")}
            on={settings.olderReadings}
            disabled={!settings.bring}
            onChange={(olderReadings) =>
              void (olderReadings ? ask : save)({ ...settings, olderReadings })
            }
          />
        )}
        {lastSent && !alone && (
          <p className="ph-hint">
            {t("phone.health.lastSent", { time: sentAt(lastSent.at), count: lastSent.count })}
          </p>
        )}
        <p className="ph-hint">{t("phone.health.fixed")}</p>
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
