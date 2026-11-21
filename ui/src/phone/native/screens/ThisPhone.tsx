import { useEffect, useState } from "react";
import { t } from "../../../copy/t";
import type { Connection, PairedDevice } from "../../engine";
import { BackBar, Button, Page } from "../../screens/ui";
import type { NativeChannel, ThisPhoneSources } from "../api";
import { Group, Row } from "./chrome";
import { calendarValue, healthValue, offered } from "./Offer";
import { openPolicy, sentAt, useHeading } from "./parts";

/**
 * This phone: where it is connected, what it brings, when it sends, its name and version, help,
 * and Disconnect. Opened from the top bar on every tab.
 */
export function ThisPhone({
  channel,
  device,
  connection,
  version,
  waiting,
  onBack,
  onCalendar,
  onHealth,
  onDisconnect,
}: {
  channel: NativeChannel;
  device: PairedDevice;
  connection: Connection;
  /** This app's version. */
  version: string;
  waiting: number;
  onBack(): void;
  onCalendar(): void;
  onHealth(): void;
  onDisconnect(): Promise<void>;
}) {
  const [sources, setSources] = useState<ThisPhoneSources | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const heading = useHeading(confirming ? "confirm" : "phone");
  const connected = connection.kind === "connected";

  useEffect(() => {
    let gone = false;
    channel.sources().then(
      // What the computer takes comes in its hello: until then nothing reads as not taken.
      (s) => !gone && setSources(connected ? s : { ...s, computerTakes: ["calendar", "health"] }),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [channel, connected]);

  const hello = connection.kind === "connected" ? connection.hello : null;
  const computer = hello?.computerName ?? device.computerName ?? t("phone.home.computer");
  const mostPrivate = (hello?.connection ?? device.connection) === "mostPrivate";
  const computerVersion = hello?.version ?? device.version;
  const last = sources?.lastSent;

  if (confirming) {
    return (
      <Page bar={<BackBar title={t("phone.sheet.title")} onBack={() => setConfirming(false)} />}>
        <main className="ph-main">
          <h2 className="ph-h1" tabIndex={-1} ref={heading}>
            {t("phone.sheet.confirm.title")}
          </h2>
          <p className="ph-body">{t("phone.sheet.confirm.body")}</p>
          {waiting > 0 && (
            <p className="ph-hint ph-hint--ink2">
              {t("phone.sheet.confirm.waiting", { count: waiting })}
            </p>
          )}
          <div className="ph-spacer" />
          <div className="ph-stack">
            <Button
              kind="danger"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void onDisconnect();
              }}
            >
              {t("phone.sheet.confirm.yes")}
            </Button>
            <Button kind="quiet" onClick={() => setConfirming(false)}>
              {t("phone.sheet.confirm.no")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  return (
    <Page bar={<BackBar title={t("phone.sheet.title")} onBack={onBack} />}>
      <main className="ph-main ph-settings">
        <h2 className="sr-only" tabIndex={-1} ref={heading}>
          {t("phone.sheet.title")}
        </h2>
        <Group title={t("phone.sheet.computer")}>
          <Row
            icon="laptop"
            label={computer}
            hint={mostPrivate ? t("phone.sheet.mostPrivate") : t("phone.sheet.simple")}
          />
        </Group>
        {sources ? (
          <Group
            title={t("phone.app.settings.brings")}
            foot={
              last
                ? t("phone.app.settings.lastSent", {
                    time: sentAt(last.at),
                    changes: last.calendarChanges,
                    readings: last.readings,
                  })
                : undefined
            }
          >
            <Row
              icon="calendar"
              label={t("phone.calendar.label")}
              value={calendarValue(sources)}
              onPress={offered(sources, "calendar") ? onCalendar : undefined}
            />
            <Row
              icon="heart"
              label={t("phone.health.label")}
              value={healthValue(sources)}
              onPress={offered(sources, "health") ? onHealth : undefined}
            />
          </Group>
        ) : null}
        <Group title={t("phone.app.settings.whenSends")}>
          <Row
            label={t("phone.app.settings.whileOpen")}
            hint={t("phone.app.settings.whileOpen.hint")}
            value={t("phone.app.settings.always")}
          />
        </Group>
        <Group title={t("phone.sheet.title")}>
          <Row
            label={t("phone.sheet.name")}
            hint={t("phone.sheet.rename")}
            value={hello?.deviceName ?? device.deviceName}
          />
          <Row
            label={t("phone.sheet.version")}
            hint={
              computerVersion
                ? t("phone.app.settings.version.computer", { version: computerVersion })
                : undefined
            }
            value={t("phone.sheet.versionValue", { version })}
          />
        </Group>
        <Group title={t("phone.app.settings.help")}>
          <Row label={t("phone.sources.privacy")} onPress={openPolicy} />
          <Row label={t("phone.app.settings.can")} hint={t("phone.sheet.can")} />
        </Group>
        <Group>
          <Row label={t("phone.sheet.disconnect")} danger onPress={() => setConfirming(true)} />
        </Group>
      </main>
    </Page>
  );
}
