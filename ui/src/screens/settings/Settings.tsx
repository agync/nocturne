import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react";
import { BackToAnswer } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { Toggle } from "../../components/Toggle/Toggle";
import { paceDays } from "../../copy/pace";
import { t } from "../../copy/t";
import type {
  Delivery,
  LettingGo,
  ModelsView,
  OnlineView,
  RemoteView,
  SettingsPatch,
  SettingsView,
  Theme,
} from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { useEngine, useSettings } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { useRemote } from "../away/useRemote";
import { guideHref } from "../guide/links";
import { useModels } from "../models/useModels";
import { useOnline } from "../online/useOnline";
import { FasterNote } from "./FasterNote";
import { patch } from "./patch";
import styles from "./Settings.module.css";
import { ShortcutField } from "./ShortcutField";
import { SPEEDS, VOICES, voiceName } from "./voices";

/** Each control is read with the row's hint, so the hint is not only for people who can see it. */
function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: (hintId: string) => ReactNode;
}) {
  const hintId = useId();
  return (
    <li className={styles.row}>
      <div className={styles.text}>
        <span className={styles.label}>{label}</span>
        <span id={hintId} className={styles.hint}>
          {hint}
        </span>
      </div>
      <div className={styles.control}>{children(hintId)}</div>
    </li>
  );
}

function Select<V extends string | number>({
  label,
  value,
  options,
  onChange,
  describedBy,
}: {
  label: string;
  value: V;
  options: ReadonlyArray<{ value: V; label: string }>;
  onChange: (value: V) => void;
  describedBy?: string;
}) {
  const id = useId();
  return (
    <select
      id={id}
      className={styles.select}
      aria-label={label}
      aria-describedby={describedBy}
      value={String(value)}
      onChange={(event) => {
        const picked = options.find((o) => String(o.value) === event.target.value);
        if (picked) onChange(picked.value);
      }}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** The words on the Online answers row: Off, On or Paused. */
export function onlineValue(view: OnlineView): string {
  switch (view.state.type) {
    case "off":
      return t("online.row.off");
    case "on":
    case "checking":
      return t("online.row.on");
    case "paused":
    case "problem":
      return t("online.row.paused");
  }
}

const TIER_NAMES = {
  large: "setup.tier.large",
  standard: "setup.tier.standard",
  small: "setup.tier.small",
  tiny: "setup.tier.tiny",
} as const;

/** The words on the row for choosing what runs each job: the tier, Custom, or Needs you. */
export function modelsValue(view: ModelsView): string {
  if (view.attention) return t("models.row.needsYou");
  if (view.custom || view.preset === "unsupported") return t("models.row.custom");
  return t("models.row.preset", { tier: t(TIER_NAMES[view.preset]) });
}

/** The words on the Away from home row: Off, or how it is on and with how many phones. */
export function awayValue(view: RemoteView): string {
  const connection = t(
    view.connection === "simple" ? "away.connection.simple" : "away.connection.mostPrivate",
  );
  switch (view.state.type) {
    case "off":
      return t("away.row.off");
    case "on":
      return view.devices.length === 0
        ? t("away.row.onNoPhones", { connection })
        : t("away.row.on", { connection, count: view.devices.length });
    case "needsRelink":
      return t("away.row.relink");
    case "problem":
      return t("away.row.problem");
    default:
      return t("away.row.setting");
  }
}

/** #/settings: nobody has to visit. Every change is saved as it is made. */
export function Settings() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const client = useQueryClient();
  const query = useSettings();
  const away = useRemote().data;
  const online = useOnline().data;
  const models = useModels().data;
  const [saved, setSaved] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const view = query.data;

  /** Shows the change at once, saves it, and puts things back if the engine says no. */
  const save = useCallback(
    async (fields: Partial<SettingsPatch>) => {
      const before = client.getQueryData<SettingsView>(["settings"]);
      if (!before) return;
      const optimistic = { ...before };
      for (const [key, value] of Object.entries(fields)) {
        if (value !== null && value !== undefined)
          (optimistic as Record<string, unknown>)[key] = value;
      }
      client.setQueryData(["settings"], optimistic);
      try {
        const next = await engine.updateSettings(patch(fields));
        client.setQueryData(["settings"], next);
        void client.invalidateQueries({ queryKey: ["privacy"] });
        setSaved(true);
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(() => setSaved(false), 2500);
      } catch (error) {
        client.setQueryData(["settings"], before);
        // The one refusal with something to do: a Mac running Nocturne from the disk image.
        const moveFirst =
          fields.startAtLogin !== undefined &&
          error instanceof EngineError &&
          error.detail === "move-to-applications";
        toast.show({ message: t(moveFirst ? "settings.login.move" : "settings.failed") });
      }
    },
    [client, engine, toast],
  );

  if (query.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("settings.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  if (!view) {
    return (
      <p className={styles.message} role="status">
        {t("settings.loading")}
      </p>
    );
  }

  // A list of 24-hour times in quarter hours: the same on every computer, whatever its clock style.
  const slots = Array.from(
    { length: 96 },
    (_, i) =>
      `${String(Math.floor(i / 4)).padStart(2, "0")}:${String((i % 4) * 15).padStart(2, "0")}`,
  );
  const times = (slots.includes(view.pageTime) ? slots : [...slots, view.pageTime].sort()).map(
    (v) => ({ value: v, label: v }),
  );
  const reminder = view.eveningReminder ?? { on: false, time: "21:00" };
  const reminderTimes = (
    slots.includes(reminder.time) ? slots : [...slots, reminder.time].sort()
  ).map((v) => ({ value: v, label: v }));
  const deliveries: Array<{ value: Delivery; label: string }> = [
    { value: "notify", label: t("settings.delivery.notify") },
    { value: "quiet", label: t("settings.delivery.quiet") },
  ];
  const voices = VOICES.some((v) => v.id === view.voice)
    ? VOICES.map((v) => ({ value: v.id, label: v.name }))
    : [
        { value: view.voice, label: voiceName(view.voice) },
        ...VOICES.map((v) => ({ value: v.id, label: v.name })),
      ];
  const speeds = (
    SPEEDS.includes(view.voiceSpeed) ? SPEEDS : [...SPEEDS, view.voiceSpeed].sort()
  ).map((s) => ({ value: s, label: `${s}×` }));
  const pacing: Array<{ value: LettingGo; label: string }> = [
    { value: "gentle", label: t("settings.letting.gentle", { days: paceDays("gentle").fade }) },
    { value: "normal", label: t("settings.letting.normal", { days: paceDays("normal").fade }) },
    { value: "brisk", label: t("settings.letting.brisk", { days: paceDays("brisk").fade }) },
  ];
  const themes: Array<{ value: Theme; label: string }> = [
    { value: "system", label: t("settings.theme.system") },
    { value: "light", label: t("theme.light") },
    { value: "dark", label: t("theme.dark") },
  ];

  return (
    <ScreenSheet>
      <BackToAnswer />
      <div className={styles.head}>
        <h1 className="t-page-title">{t("settings.title")}</h1>
        <p className={styles.saved} role="status">
          {saved ? t("settings.saved") : ""}
        </p>
      </div>
      <ul className={styles.rows}>
        <Row label={t("settings.page.label")} hint={t("settings.page.hint")}>
          {(hint) => (
            <>
              <Select
                describedBy={hint}
                label={t("settings.page.time")}
                value={view.pageTime}
                options={times}
                onChange={(pageTime) => void save({ pageTime })}
              />
              <Select
                describedBy={hint}
                label={t("settings.page.delivery")}
                value={view.delivery}
                options={deliveries}
                onChange={(delivery) => void save({ delivery })}
              />
            </>
          )}
        </Row>

        <Row label={t("settings.voice.label")} hint={t("settings.voice.hint")}>
          {(hint) => (
            <>
              <Select
                describedBy={hint}
                label={t("settings.voice.name")}
                value={view.voice}
                options={voices}
                onChange={(voice) => void save({ voice })}
              />
              <Select
                describedBy={hint}
                label={t("settings.voice.speed")}
                value={view.voiceSpeed}
                options={speeds}
                onChange={(voiceSpeed) => void save({ voiceSpeed })}
              />
            </>
          )}
        </Row>

        <Row label={t("settings.shortcut.label")} hint={t("settings.shortcut.hint")}>
          {(hint) => (
            <>
              <ShortcutField
                describedBy={hint}
                value={view.shortcut}
                available={view.shortcutAvailable}
                desktop={view.desktopShortcut}
                onChange={(shortcut) => void save({ shortcut })}
              />
            </>
          )}
        </Row>

        <Row label={t("settings.inbox.label")} hint={t("settings.inbox.hint")}>
          {(hint) => (
            <>
              <span className={view.inboxFolder ? styles.path : styles.none}>
                {view.inboxFolder ?? t("settings.inbox.none")}
              </span>
              <Button
                variant="quiet"
                aria-describedby={hint}
                onClick={() => {
                  void platform.pickFolder().then((dir) => {
                    if (dir) void save({ inboxFolder: dir });
                  });
                }}
              >
                {t("settings.inbox.choose")}
              </Button>
              {view.inboxFolder ? (
                <Button
                  variant="quiet"
                  aria-label={t("settings.inbox.stop.named")}
                  onClick={() => void save({ inboxFolder: "" })}
                >
                  {t("settings.inbox.stop")}
                </Button>
              ) : null}
            </>
          )}
        </Row>

        <li className={styles.toggleRow}>
          <Toggle
            label={t("settings.login.label")}
            hint={t("settings.login.hint")}
            checked={view.startAtLogin}
            onChange={(startAtLogin) => void save({ startAtLogin })}
          />
        </li>

        <li className={styles.toggleRow}>
          <Toggle
            label={t("journal.reminder.label")}
            hint={t("journal.reminder.hint")}
            checked={reminder.on}
            onChange={(on) => void save({ eveningReminder: { on, time: reminder.time } })}
          />
          {reminder.on ? (
            <div className={styles.reminderTime}>
              <Select
                label={t("journal.reminder.label")}
                value={reminder.time}
                options={reminderTimes}
                onChange={(time) => void save({ eveningReminder: { on: true, time } })}
              />
            </div>
          ) : null}
        </li>

        <Row label={t("settings.letting.label")} hint={t("settings.letting.hint")}>
          {(hint) => (
            <>
              <Select
                describedBy={hint}
                label={t("settings.letting.label")}
                value={view.lettingGo}
                options={pacing}
                onChange={(lettingGo) => void save({ lettingGo })}
              />
            </>
          )}
        </Row>

        <Row label={t("settings.theme.label")} hint={t("settings.theme.hint")}>
          {(hint) => (
            <>
              <Select
                describedBy={hint}
                label={t("settings.theme.name")}
                value={view.theme}
                options={themes}
                onChange={(theme) => void save({ theme })}
              />
              <span className={styles.value}>{t("settings.language.value")}</span>
            </>
          )}
        </Row>

        {away ? (
          <Row label={t("away.row.label")} hint={t("away.row.hint")}>
            {(hint) => (
              <a
                className={`${styles.value} hit`}
                href="#/settings/away"
                aria-describedby={hint}
                aria-label={t("away.row.open", { value: awayValue(away) })}
              >
                {awayValue(away)} ›
              </a>
            )}
          </Row>
        ) : null}

        {online ? (
          <Row label={t("online.row.label")} hint={t("online.row.hint")}>
            {(hint) => (
              <a
                className={`${styles.value} hit`}
                href="#/settings/online"
                aria-describedby={hint}
                aria-label={t("online.row.open", { value: onlineValue(online) })}
              >
                {onlineValue(online)} ›
              </a>
            )}
          </Row>
        ) : null}

        {models ? (
          <Row
            label={t("models.row.label")}
            hint={models.attention ? t("models.row.attention") : t("models.row.hint")}
          >
            {(hint) => (
              <a
                className={`${styles.value} hit`}
                href="#/settings/models"
                aria-describedby={hint}
                aria-label={t("models.row.open", { value: modelsValue(models) })}
              >
                {modelsValue(models)} ›
              </a>
            )}
          </Row>
        ) : null}

        <Row label={t("guide.row.label")} hint={t("guide.row.hint")}>
          {(hint) => (
            <a
              className={`${styles.value} hit`}
              href={guideHref("settings")}
              aria-describedby={hint}
              aria-label={t("guide.row.open", { value: t("guide.row.value") })}
            >
              {t("guide.row.value")} ›
            </a>
          )}
        </Row>
      </ul>
      <FasterNote />
      <a className={`${styles.advanced} hit`} href="#/settings/advanced">
        {t("settings.advanced.show")}
      </a>
    </ScreenSheet>
  );
}
