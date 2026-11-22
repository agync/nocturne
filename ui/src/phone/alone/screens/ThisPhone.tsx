import { useEffect, useState } from "react";
import { formatBytes, formatLongDay, formatTime12 } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { HomeStats, PutAsideMemory } from "../../../engine/bindings";
import type { NativeChannel, ThisPhoneSources } from "../../native/api";
import { Group, Row } from "../../native/screens/chrome";
import { calendarValue, healthValue } from "../../native/screens/Offer";
import { openPolicy, useHeading } from "../../native/screens/parts";
import { openAppSettings } from "../../native/sources";
import { BackBar, Page } from "../../screens/ui";
import type { VoiceAccess } from "../api";

/** On a phone alone nothing is sent to a computer: the phone takes what it brings itself. */
export function aloneSources(s: ThisPhoneSources): ThisPhoneSources {
  const own = (status: ThisPhoneSources["calendar"]) =>
    status === "stoppedOnComputer" || status === "needsNewerComputer" ? "notBrought" : status;
  return {
    ...s,
    calendar: own(s.calendar),
    health: own(s.health),
    computerTakes: ["calendar", "health"],
  };
}

/** Two put aside on the same day: each says its time too. */
export function sameDay(m: PutAsideMemory, all: readonly PutAsideMemory[]): boolean {
  const day = formatLongDay(m.at);
  return all.some((o) => o.id !== m.id && formatLongDay(o.at) === day);
}

function putAsideLabel(m: PutAsideMemory, all: readonly PutAsideMemory[]): string {
  const day = formatLongDay(m.at);
  return sameDay(m, all)
    ? t("phone.alone.thisPhone.putAsideAt", { day, time: formatTime12(m.at) })
    : t("phone.alone.thisPhone.putAside", { day });
}

/**
 * This phone on a phone alone: where the memory is and its mind, what the phone brings, its
 * name and what it can do, then moving, a backup and deleting everything.
 */
export function HomeThisPhone({
  channel,
  stats,
  version,
  voice,
  onBack,
  onMind,
  onCalendar,
  onHealth,
  onBackup,
  onRestore,
  onDelete,
  putAside,
  onPutAside,
  onDeleteKept,
  onPermissions,
  nearlyFull,
}: {
  channel: NativeChannel;
  stats(): Promise<HomeStats>;
  /** The memories put aside on this phone, newest first. */
  putAside(): Promise<PutAsideMemory[]>;
  /** `withTime`: another was put aside the same day, so the time tells them apart. */
  onPutAside(memory: PutAsideMemory, withTime: boolean): void;
  /** "Delete what was kept…" from the calendar or Health. */
  onDeleteKept(): void;
  /** "Your phone's permissions", and where to take them back. */
  onPermissions(): void;
  /** Less than 500 MB is free. */
  nearlyFull: boolean;
  /** This app's version. */
  version: string;
  /** Whether voice notes can be read into words, and the way to turn it on. */
  voice: {
    access(): Promise<VoiceAccess>;
    turnOn(): Promise<VoiceAccess>;
    /** The line before the phone's prompt was shown, so the row has a reason to be here. */
    asked(): Promise<boolean>;
  };
  onBack(): void;
  onMind(): void;
  onCalendar(): void;
  onHealth(): void;
  onBackup(): void;
  onRestore(): void;
  onDelete(): void;
}) {
  const [sources, setSources] = useState<ThisPhoneSources | null>(null);
  const [held, setHeld] = useState<HomeStats | null>(null);
  const [heard, setHeard] = useState<VoiceAccess | null>(null);
  const [aside, setAside] = useState<PutAsideMemory[]>([]);
  const heading = useHeading("thisPhone");

  useEffect(() => {
    let gone = false;
    channel.sources().then(
      (s) => !gone && setSources(aloneSources(s)),
      () => undefined,
    );
    stats().then(
      (s) => !gone && setHeld(s),
      () => undefined,
    );
    putAside().then(
      (list) => !gone && setAside(list),
      () => undefined,
    );
    Promise.all([voice.access(), voice.asked()]).then(
      ([a, asked]) => !gone && setHeard(asked || a === "denied" ? a : "granted"),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [channel, stats, voice, putAside]);

  const last = held?.lastBackup ?? null;
  return (
    <Page bar={<BackBar title={t("phone.sheet.title")} onBack={onBack} />}>
      <main className="ph-main ph-settings">
        <h2 className="sr-only" tabIndex={-1} ref={heading}>
          {t("phone.sheet.title")}
        </h2>
        <Group title={t("phone.alone.thisPhone.memory")}>
          <Row
            icon="phone"
            label={t("phone.alone.thisPhone.where")}
            hint={
              held
                ? t("phone.alone.thisPhone.whereValue", { size: formatBytes(held.bytes) })
                : undefined
            }
          />
          {nearlyFull ? (
            <p className="ph-srow-hint ph-full-line" role="status">
              {t("phone.alone.full.title")} {t("phone.alone.full.body")}
            </p>
          ) : null}
          <Row
            icon="moon"
            label={t("phone.alone.thisPhone.mind")}
            hint={t("phone.alone.thisPhone.mindNone")}
            onPress={onMind}
          />
          {aside.map((m) => (
            <Row
              key={m.id}
              label={putAsideLabel(m, aside)}
              onPress={() => onPutAside(m, sameDay(m, aside))}
            />
          ))}
        </Group>
        {sources ? (
          <Group title={t("phone.app.settings.brings")}>
            <Row
              icon="calendar"
              label={t("phone.calendar.label")}
              value={calendarValue(sources)}
              onPress={onCalendar}
            />
            <Row
              icon="heart"
              label={t("phone.health.label")}
              value={healthValue(sources)}
              onPress={onHealth}
            />
            <Row label={t("phone.alone.sources.delete")} onPress={onDeleteKept} />
            {heard === "denied" ? (
              <Row
                icon="mic"
                label={t("phone.alone.voice.row")}
                hint={t("phone.alone.voice.offInSettings")}
                value={t("phone.sources.openPhoneSettings")}
                onPress={() => void openAppSettings()}
              />
            ) : heard && heard !== "granted" ? (
              <Row
                icon="mic"
                label={t("phone.alone.voice.row")}
                hint={t("phone.alone.voice.off")}
                value={t("phone.alone.voice.turnOn")}
                onPress={() => void voice.turnOn().then(setHeard)}
              />
            ) : null}
          </Group>
        ) : null}
        <Group title={t("phone.sheet.title")}>
          <Row
            label={t("phone.sheet.version")}
            value={t("phone.sheet.versionValue", { version })}
          />
        </Group>
        <Group title={t("phone.app.settings.help")}>
          <Row label={t("phone.sources.privacy")} onPress={openPolicy} />
          <Row label={t("phone.alone.permissions.title")} onPress={onPermissions} />
          <Row
            label={t("phone.app.settings.can")}
            hint={`${t("phone.alone.thisPhone.can")} ${t("phone.alone.thisPhone.removing")}`}
          />
        </Group>
        <Group>
          <Row label={t("phone.alone.thisPhone.move")} onPress={onMind} />
          <Row
            label={t("phone.alone.thisPhone.save")}
            hint={
              last
                ? t("phone.alone.thisPhone.lastBackup", { day: formatLongDay(last.at) })
                : t("phone.alone.thisPhone.noBackup")
            }
            onPress={onBackup}
          />
          <Row
            label={t("phone.alone.thisPhone.restore")}
            hint={t("phone.alone.thisPhone.restoreHint")}
            onPress={onRestore}
          />
          <Row label={t("phone.alone.thisPhone.delete")} danger onPress={onDelete} />
        </Group>
      </main>
    </Page>
  );
}
