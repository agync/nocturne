import { useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { Toggle } from "../../components/Toggle/Toggle";
import { formatBytes } from "../../copy/format";
import { t } from "../../copy/t";
import type { SettingsPatch, SettingsView } from "../../engine/bindings";
import { useEngine, useSettings, useStatus } from "../../engine/context";
import { patch } from "../settings/patch";
import { ShortcutField } from "../settings/ShortcutField";
import styles from "./Setup.module.css";

const SLOTS = Array.from(
  { length: 96 },
  (_, i) =>
    `${String(Math.floor(i / 4)).padStart(2, "0")}:${String((i % 4) * 15).padStart(2, "0")}`,
);

/** The right-hand panel of H3: good defaults to change now or never, and what happens tonight. */
export function Defaults() {
  const engine = useEngine();
  const toast = useToast();
  const client = useQueryClient();
  const { data: view } = useSettings();
  const { data: status } = useStatus();
  const [timing, setTiming] = useState(false);
  const select = useRef<HTMLSelectElement>(null);

  const save = async (fields: Partial<SettingsPatch>) => {
    const before = client.getQueryData<SettingsView>(["settings"]);
    if (!before) return;
    client.setQueryData(["settings"], { ...before, ...fields });
    try {
      client.setQueryData(["settings"], await engine.updateSettings(patch(fields)));
    } catch {
      client.setQueryData(["settings"], before);
      toast.show({ message: t("settings.failed") });
    }
  };

  const sent = !status
    ? null
    : !status.sentMeasured
      ? t("status.internet.unmeasured")
      : status.sentToInternetBytes > 0
        ? t("status.internet.amount", { size: formatBytes(status.sentToInternetBytes) })
        : t("status.internet.nothing");

  return (
    <aside className={styles.defaults} aria-labelledby="defaults">
      <h2 id="defaults" className={`t-section ${styles.eyebrow}`}>
        {t("setup.defaults.title")}
      </h2>
      {view ? (
        <ul className={styles.defaultRows}>
          <li>
            <span className={styles.defaultLabel}>{t("setup.defaults.page")}</span>
            <span className={styles.defaultValue}>
              {timing ? (
                <select
                  ref={select}
                  className={styles.select}
                  aria-label={t("setup.defaults.page.time")}
                  value={view.pageTime}
                  onChange={(event) => void save({ pageTime: event.target.value })}
                  onBlur={() => setTiming(false)}
                >
                  {(SLOTS.includes(view.pageTime) ? SLOTS : [...SLOTS, view.pageTime].sort()).map(
                    (v) => (
                      <option key={v} value={v}>
                        {v}
                      </option>
                    ),
                  )}
                </select>
              ) : (
                <>
                  <span className={styles.mono}>{view.pageTime}</span>
                  <Button
                    variant="quiet"
                    aria-label={t("setup.defaults.page.change.named")}
                    onClick={() => {
                      setTiming(true);
                      setTimeout(() => select.current?.focus(), 0);
                    }}
                  >
                    {t("setup.defaults.page.change")}
                  </Button>
                </>
              )}
            </span>
          </li>
          <li>
            <span className={styles.defaultLabel}>{t("setup.defaults.shortcut")}</span>
            <ShortcutField
              value={view.shortcut}
              available={view.shortcutAvailable}
              desktop={view.desktopShortcut}
              onChange={(shortcut) => void save({ shortcut })}
            />
          </li>
          <li className={styles.toggle}>
            <Toggle
              label={t("setup.defaults.login")}
              hint={t("setup.defaults.login.hint")}
              checked={view.startAtLogin}
              onChange={(startAtLogin) => void save({ startAtLogin })}
            />
          </li>
        </ul>
      ) : null}
      <section className={styles.tonight} aria-labelledby="tonight">
        <h3 id="tonight" className={styles.defaultLabel}>
          {t("setup.tonight.title")}
        </h3>
        <p>{t("setup.tonight.body")}</p>
      </section>
      {sent ? (
        <p className={styles.sent}>
          {t("status.internet.label")}: <strong>{sent}</strong>
        </p>
      ) : null}
    </aside>
  );
}
