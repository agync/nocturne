import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { Toggle } from "../../components/Toggle/Toggle";
import { t } from "../../copy/t";
import type {
  Delivery,
  LettingGo,
  SettingsPatch,
  SettingsView,
  Theme,
} from "../../engine/bindings";
import { useEngine, useSettings } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { patch } from "./patch";
import styles from "./Settings.module.css";
import { ShortcutField } from "./ShortcutField";
import { SPEEDS, VOICES, voiceName } from "./voices";

function Row({ label, hint, children }: { label: string; hint: string; children: ReactNode }) {
  return (
    <li className={styles.row}>
      <div className={styles.text}>
        <span className={styles.label}>{label}</span>
        <span className={styles.hint}>{hint}</span>
      </div>
      <div className={styles.control}>{children}</div>
    </li>
  );
}

function Select<V extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: V;
  options: ReadonlyArray<{ value: V; label: string }>;
  onChange: (value: V) => void;
}) {
  const id = useId();
  return (
    <select
      id={id}
      className={styles.select}
      aria-label={label}
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

/** #/settings: nobody has to visit. Every change is saved as it is made. */
export function Settings() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const client = useQueryClient();
  const query = useSettings();
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
      } catch {
        client.setQueryData(["settings"], before);
        toast.show({ message: t("settings.failed") });
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
    { value: "gentle", label: t("settings.letting.gentle") },
    { value: "normal", label: t("settings.letting.normal") },
    { value: "brisk", label: t("settings.letting.brisk") },
  ];
  const themes: Array<{ value: Theme; label: string }> = [
    { value: "system", label: t("settings.theme.system") },
    { value: "light", label: t("theme.light") },
    { value: "dark", label: t("theme.dark") },
  ];

  return (
    <div className={styles.frame}>
      <div className={styles.card}>
        <div className={styles.head}>
          <h1 className="t-page-title">{t("settings.title")}</h1>
          <p className={styles.saved} role="status">
            {saved ? t("settings.saved") : ""}
          </p>
        </div>
        <ul className={styles.rows}>
          <Row label={t("settings.page.label")} hint={t("settings.page.hint")}>
            <Select
              label={t("settings.page.time")}
              value={view.pageTime}
              options={times}
              onChange={(pageTime) => void save({ pageTime })}
            />
            <Select
              label={t("settings.page.delivery")}
              value={view.delivery}
              options={deliveries}
              onChange={(delivery) => void save({ delivery })}
            />
          </Row>

          <li className={styles.toggleRow}>
            <Toggle
              label={t("settings.print.label")}
              hint={t("settings.print.hint")}
              checked={view.autoPrint}
              onChange={(autoPrint) => void save({ autoPrint })}
            />
          </li>

          <Row label={t("settings.voice.label")} hint={t("settings.voice.hint")}>
            <Select
              label={t("settings.voice.name")}
              value={view.voice}
              options={voices}
              onChange={(voice) => void save({ voice })}
            />
            <Select
              label={t("settings.voice.speed")}
              value={view.voiceSpeed}
              options={speeds}
              onChange={(voiceSpeed) => void save({ voiceSpeed })}
            />
          </Row>

          <Row label={t("settings.shortcut.label")} hint={t("settings.shortcut.hint")}>
            <ShortcutField
              value={view.shortcut}
              available={view.shortcutAvailable}
              onChange={(shortcut) => void save({ shortcut })}
            />
          </Row>

          <Row label={t("settings.inbox.label")} hint={t("settings.inbox.hint")}>
            <span className={view.inboxFolder ? styles.path : styles.none}>
              {view.inboxFolder ?? t("settings.inbox.none")}
            </span>
            <Button
              variant="quiet"
              onClick={() => {
                void platform.pickFolder().then((dir) => {
                  if (dir) void save({ inboxFolder: dir });
                });
              }}
            >
              {t("settings.inbox.choose")}
            </Button>
          </Row>

          <li className={styles.toggleRow}>
            <Toggle
              label={t("settings.login.label")}
              hint={t("settings.login.hint")}
              checked={view.startAtLogin}
              onChange={(startAtLogin) => void save({ startAtLogin })}
            />
          </li>

          <Row label={t("settings.letting.label")} hint={t("settings.letting.hint")}>
            <Select
              label={t("settings.letting.label")}
              value={view.lettingGo}
              options={pacing}
              onChange={(lettingGo) => void save({ lettingGo })}
            />
          </Row>

          <Row label={t("settings.theme.label")} hint={t("settings.theme.hint")}>
            <Select
              label={t("settings.theme.name")}
              value={view.theme}
              options={themes}
              onChange={(theme) => void save({ theme })}
            />
            <span className={styles.value}>{t("settings.language.value")}</span>
          </Row>

          <li className={styles.toggleRow}>
            <Toggle
              label={t("settings.updates.title")}
              hint={t(view.checkUpdates ? "settings.updates.on" : "settings.updates.off")}
              checked={view.checkUpdates}
              onChange={(checkUpdates) => void save({ checkUpdates })}
            />
          </li>

          <li className={styles.toggleRow}>
            <Toggle
              label={t("settings.reminders.label")}
              hint={t("settings.reminders.hint")}
              checked={view.reminders}
              onChange={(reminders) => void save({ reminders })}
            />
          </li>
        </ul>
        <a className={styles.advanced} href="#/settings/advanced">
          {t("settings.advanced.show")}
        </a>
      </div>
    </div>
  );
}
