import { useEffect, useState } from "react";
import { formatBytes, formatLongDay, formatTimeOfDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { HomeStats, PutAsideMemory } from "../../../engine/bindings";
import { usdText } from "../../../screens/online/limits";
import type { NativeChannel, ThisPhoneSources } from "../../native/api";
import { Group, Row } from "../../native/screens/chrome";
import { calendarValue, healthValue } from "../../native/screens/Offer";
import { openPolicy, useHeading } from "../../native/screens/parts";
import { openAppSettings } from "../../native/sources";
import { BottomSheet } from "../../screens/Dialog";
import { BackBar, Button, Page } from "../../screens/ui";
import type { VoiceAccess } from "../api";
import type { MindChosen, MindLimit, MindMonth, MindReading } from "../mind";
import type { BackupStatus } from "../nightly";
import { backupRowHint } from "./Nightly";

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
    ? t("phone.alone.thisPhone.putAsideAt", { day, time: formatTimeOfDay(m.at) })
    : t("phone.alone.thisPhone.putAside", { day });
}

function limitValue(limit: MindLimit): string {
  return limit.kind === "amount"
    ? usdText(limit.usd)
    : t("phone.mind.row.limitThings", { count: limit.kind === "things" ? limit.count : 0 });
}

/** "Nocturne's mind online": the chosen service, what goes, what was sent, and turning off. */
export function MindGroup({
  chosen,
  month,
  reading,
  fresh,
  onSent,
  onChange,
  onCheckKey,
  onTurnOff,
}: {
  chosen: Exclude<MindChosen, { state: "off" }>;
  month(): Promise<MindMonth | null>;
  reading(): Promise<MindReading>;
  /** Changes whenever the mind has read or written, so the counts are looked at again. */
  fresh: number;
  onSent(): void;
  onChange(): void;
  onCheckKey(): void;
  onTurnOff(): Promise<void>;
}) {
  const [counted, setCounted] = useState<MindMonth | null>(null);
  const [read, setRead] = useState<MindReading | null>(null);
  const [asking, setAsking] = useState(false);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let gone = false;
    month().then(
      (m) => !gone && setCounted(m),
      () => undefined,
    );
    reading().then(
      (r) => !gone && setRead(r),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [month, reading, fresh]);

  const turnOff = async () => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await onTurnOff();
      setAsking(false);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const host = chosen.host;
  const underWay = read && read.count > 0 && read.done < read.count ? read : null;
  return (
    <Group title={t("phone.mind.group")}>
      {chosen.state === "needsKey" ? (
        <div className="ph-mind-nokey" role="status">
          <p className="ph-error-title">{t("phone.mind.noKey.title")}</p>
          <p className="ph-hint ph-hint--ink2">{t("phone.mind.noKey.body", { host })}</p>
          <button type="button" className="ph-link" onClick={onCheckKey}>
            {t("phone.mind.noKey.action")}
          </button>
        </div>
      ) : null}
      <Row
        icon="moon"
        label={t("phone.mind.row.mind")}
        hint={[
          t("phone.mind.row.mindValue", { host }),
          underWay
            ? t("phone.mind.row.reading", { done: underWay.done, count: underWay.count })
            : null,
        ]
          .filter(Boolean)
          .join(" · ")}
      />
      <Row label={t("phone.mind.row.goes")} hint={t("phone.mind.row.goesValue")} />
      <Row label={t("phone.mind.row.sent")} onPress={onSent} />
      {counted ? (
        <Row
          label={t("phone.mind.row.month")}
          hint={
            counted.limit.kind === "none"
              ? t("phone.mind.row.monthNone", { count: counted.count })
              : t("phone.mind.row.monthLimit", {
                  count: counted.count,
                  limit: limitValue(counted.limit),
                })
          }
        />
      ) : null}
      <Row label={t("phone.mind.row.change")} onPress={onChange} />
      <Row label={t("phone.mind.row.turnOff")} danger onPress={() => setAsking(true)} />
      {asking ? (
        <BottomSheet label={t("phone.mind.off.title")} onClose={() => setAsking(false)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 16 }}>
            <h2 className="ph-h1 ph-h1--small">{t("phone.mind.off.title")}</h2>
            <p className="ph-body">{t("phone.mind.off.body")}</p>
            {failed ? (
              <p className="ph-error" role="alert">
                {t("phone.mind.off.failed")}
              </p>
            ) : null}
            <Button kind="danger" disabled={busy} onClick={() => void turnOff()}>
              {t("phone.mind.off.yes")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
              {t("phone.mind.off.no")}
            </button>
          </div>
        </BottomSheet>
      ) : null}
    </Group>
  );
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
  mind,
  backup,
}: {
  /** The nightly backup: how it stands, and its screen. */
  backup?: { status(): Promise<BackupStatus>; ios: boolean; now(): number; onOpen(): void };
  /** The mind chosen, and its rows' ways out; none until it is known. */
  mind: {
    chosen: MindChosen | null;
    month(): Promise<MindMonth | null>;
    reading(): Promise<MindReading>;
    fresh: number;
    onMove(): void;
    onSent(): void;
    onCheckKey(): void;
    onTurnOff(): Promise<void>;
  };
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
  const [nightly, setNightly] = useState<BackupStatus | null>(null);
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
    backup?.status().then(
      (b) => !gone && setNightly(b),
      () => undefined,
    );
    Promise.all([voice.access(), voice.asked()]).then(
      ([a, asked]) => !gone && setHeard(asked || a === "denied" ? a : "granted"),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [channel, stats, voice, putAside, backup]);

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
          {backup ? (
            <Row
              icon="refresh"
              label={t("phone.backup.row")}
              hint={backupRowHint(nightly, backup.ios, new Date(backup.now()))}
              onPress={backup.onOpen}
            />
          ) : null}
          {nearlyFull ? (
            <p className="ph-srow-hint ph-full-line" role="status">
              {t("phone.alone.full.title")} {t("phone.alone.full.body")}
            </p>
          ) : null}
          {!mind.chosen || mind.chosen.state === "off" ? (
            <Row
              icon="moon"
              label={t("phone.alone.thisPhone.mind")}
              hint={t("phone.alone.thisPhone.mindNone")}
              onPress={onMind}
            />
          ) : null}
          {aside.map((m) => (
            <Row
              key={m.id}
              label={putAsideLabel(m, aside)}
              onPress={() => onPutAside(m, sameDay(m, aside))}
            />
          ))}
        </Group>
        {mind.chosen && mind.chosen.state !== "off" ? (
          <MindGroup
            chosen={mind.chosen}
            month={mind.month}
            reading={mind.reading}
            fresh={mind.fresh}
            onSent={mind.onSent}
            onChange={onMind}
            onCheckKey={mind.onCheckKey}
            onTurnOff={mind.onTurnOff}
          />
        ) : null}
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
          <Row label={t("phone.alone.thisPhone.move")} onPress={mind.onMove} />
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
