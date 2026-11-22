import { useEffect, useState } from "react";
import { formatBytes, formatLongDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { HomeStats } from "../../../engine/bindings";
import type { NativeChannel, ThisPhoneSources } from "../../native/api";
import { Group, Row } from "../../native/screens/chrome";
import { calendarValue, healthValue } from "../../native/screens/Offer";
import { useHeading } from "../../native/screens/parts";
import { BackBar, Page } from "../../screens/ui";

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

/**
 * This phone on a phone alone: where the memory is and its mind, what the phone brings, its
 * name and what it can do, then moving, a backup and deleting everything.
 */
export function HomeThisPhone({
  channel,
  stats,
  name,
  onBack,
  onMind,
  onCalendar,
  onHealth,
  onBackup,
  onDelete,
}: {
  channel: NativeChannel;
  stats(): Promise<HomeStats>;
  name: string;
  onBack(): void;
  onMind(): void;
  onCalendar(): void;
  onHealth(): void;
  onBackup(): void;
  onDelete(): void;
}) {
  const [sources, setSources] = useState<ThisPhoneSources | null>(null);
  const [held, setHeld] = useState<HomeStats | null>(null);
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
    return () => {
      gone = true;
    };
  }, [channel, stats]);

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
          <Row
            icon="moon"
            label={t("phone.alone.thisPhone.mind")}
            hint={t("phone.alone.thisPhone.mindNone")}
            onPress={onMind}
          />
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
          </Group>
        ) : null}
        <Group title={t("phone.sheet.title")}>
          <Row label={t("phone.sheet.name")} value={name} />
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
          <Row label={t("phone.alone.thisPhone.delete")} danger onPress={onDelete} />
        </Group>
      </main>
    </Page>
  );
}
