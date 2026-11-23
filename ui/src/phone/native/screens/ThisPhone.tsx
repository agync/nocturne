import { useCallback, useEffect, useState } from "react";
import { formatLongDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import { type AsideApi, endedFor, type MoveAside, type Moved } from "../../alone/screens/MoveFlow";
import {
  type MoveEnded,
  MoveFailed,
  MoveMoved,
  MoveMoving,
  type MoveProgress,
} from "../../alone/screens/MoveScreens";
import type { Connection, PairedDevice } from "../../engine";
import { BackBar, Button, Page } from "../../screens/ui";
import type { NativeChannel, ThisPhoneSources } from "../api";
import { Group, Row } from "./chrome";
import { calendarValue, healthValue, offered } from "./Offer";
import { openPolicy, sentAt, useHeading } from "./parts";

type AsideStep =
  | { name: "askBring" }
  | { name: "bringing" }
  | { name: "failed"; why: MoveEnded }
  | { name: "brought"; moved: Moved; busy: boolean }
  | { name: "remove"; busy: boolean };

/**
 * This phone: where it is connected, what it brings, when it sends, its name and version, help,
 * the memory a move put aside here, and Disconnect. Opened from the top bar on every tab.
 */
export function ThisPhone({
  channel,
  device,
  connection,
  version,
  waiting,
  aside,
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
  /** The memory a move put aside here, when the app can reach it. */
  aside?: AsideApi;
  onBack(): void;
  onCalendar(): void;
  onHealth(): void;
  onDisconnect(): Promise<void>;
}) {
  const [sources, setSources] = useState<ThisPhoneSources | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [put, setPut] = useState<MoveAside | null>(null);
  const [step, setStep] = useState<AsideStep | null>(null);
  const [progress, setProgress] = useState<MoveProgress | null>(null);
  const heading = useHeading(
    confirming
      ? "confirm"
      : step?.name === "remove" || step?.name === "askBring"
        ? "remove"
        : "phone",
  );
  const connected = connection.kind === "connected";

  const look = useCallback(() => {
    if (!aside) return;
    void aside.aside().then(setPut, () => setPut(null));
  }, [aside]);
  useEffect(look, [look]);

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

  const bring = async () => {
    if (!aside) return;
    setProgress(null);
    setStep({ name: "bringing" });
    try {
      const moved = await aside.bring(setProgress);
      setStep({ name: "brought", moved, busy: false });
    } catch (e) {
      // Stopped here: Stop says so itself once the computer heard.
      const why = endedFor(e);
      if (why) setStep({ name: "failed", why });
    }
  };
  const settle = (remove: boolean) => {
    if (!aside || (step?.name !== "brought" && step?.name !== "remove")) return;
    setStep({ ...step, busy: true });
    void aside
      .putAside(remove)
      .catch(() => undefined)
      .finally(() => {
        setStep(null);
        look();
      });
  };

  if (step?.name === "askBring" && put) {
    const counts = put.counts ?? null;
    const day = put.at ? formatLongDay(put.at) : null;
    return (
      <Page bar={<BackBar title={t("phone.sheet.title")} onBack={() => setStep(null)} />}>
        <main className="ph-main">
          <h2 className="ph-h1" tabIndex={-1} ref={heading}>
            {day
              ? t("moving.phone.aside.bringAsk", { day, computer })
              : t("moving.phone.moved.choice")}
          </h2>
          {counts ? (
            <p className="ph-body">
              {t("moving.phone.aside.bringBody", {
                computer,
                things: counts.things,
                entries: counts.journalEntries,
                checkins: counts.checkins,
              })}
            </p>
          ) : null}
          <div className="ph-spacer" />
          <div className="ph-stack">
            <Button onClick={() => void bring()}>{t("moving.phone.aside.bringYes")}</Button>
            <Button kind="quiet" onClick={() => setStep(null)}>
              {t("moving.phone.aside.bringNot")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }
  if (step?.name === "bringing")
    return (
      <MoveMoving
        computer={computer}
        bring
        progress={progress}
        onStop={() => {
          // Refused once the computer is adding them: this waits to hear that instead.
          void aside?.stop().then(
            () => setStep({ name: "failed", why: "stoppedHere" }),
            () => undefined,
          );
        }}
      />
    );
  if (step?.name === "failed")
    return (
      <MoveFailed
        computer={computer}
        why={step.why}
        bring
        onTryAgain={() => void bring()}
        onBack={() => setStep(null)}
      />
    );
  if (step?.name === "brought")
    return (
      <MoveMoved
        computer={computer}
        brought={step.moved.brought}
        busy={step.busy}
        onDone={settle}
      />
    );
  if (step?.name === "remove" && put) {
    // Its things are on the computer: this removes a copy. Otherwise it is the only one, and
    // removing it deletes it for good.
    const day = put.at ? formatLongDay(put.at) : null;
    const counts = put.counts ?? null;
    const title = !day
      ? t("moving.phone.moved.choice")
      : put.onComputer
        ? t("moving.phone.aside.removeAsk", { day })
        : t("moving.phone.aside.deleteAsk", { day });
    return (
      <Page bar={<BackBar title={t("phone.sheet.title")} onBack={() => setStep(null)} />}>
        <main className="ph-main">
          <h2 className="ph-h1" tabIndex={-1} ref={heading}>
            {title}
          </h2>
          {put.onComputer ? (
            <>
              <p className="ph-body">{t("moving.phone.aside.removeBody", { computer })}</p>
              {put.writing ? (
                <p className="ph-body">{t("moving.phone.aside.removeWriting")}</p>
              ) : null}
            </>
          ) : counts ? (
            <p className="ph-body">
              {t("moving.phone.aside.deleteBody", {
                computer,
                things: counts.things,
                entries: counts.journalEntries,
                media: counts.photosAndRecordings,
              })}
            </p>
          ) : (
            <p className="ph-body">{t("moving.phone.used.warning", { computer })}</p>
          )}
          <div className="ph-spacer" />
          <div className="ph-stack">
            <Button kind="danger" disabled={step.busy} onClick={() => settle(true)}>
              {t(put.onComputer ? "moving.phone.aside.removeYes" : "moving.phone.aside.deleteYes")}
            </Button>
            <Button kind="quiet" onClick={() => setStep(null)}>
              {t("moving.phone.aside.keepIt")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

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
        {put ? (
          <Group title={t("moving.phone.moved.choice")}>
            {put.onComputer ? null : (
              <Row
                label={t("moving.phone.aside.bring", { computer })}
                onPress={() => setStep({ name: "askBring" })}
              />
            )}
            <Row
              label={t("moving.phone.aside.remove")}
              danger
              onPress={() => setStep({ name: "remove", busy: false })}
            />
          </Group>
        ) : null}
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
