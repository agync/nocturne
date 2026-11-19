import { useState } from "react";
import { Dialog } from "../../components/Dialog/Dialog";
import { Tabs } from "../../components/Tabs/Tabs";
import { formatDayMonth, formatLongDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { Period, PeriodKind } from "../../engine/bindings";
import { useJournalDays, usePeriods } from "../../engine/context";
import styles from "./Journal.module.css";
import { dayDate } from "./time";

type Tab = "day" | PeriodKind;

const LABEL = {
  day: "journal.recap.tab.day",
  week: "journal.recap.tab.week",
  month: "journal.recap.tab.month",
  year: "journal.recap.tab.year",
} as const satisfies Record<Tab, Parameters<typeof t>[0]>;

const TABS: Tab[] = ["day", "week", "month", "year"];

const MONTH = new Intl.DateTimeFormat("en-GB", { month: "long" });

function periodLabel(period: Period): string {
  if (period.kind === "week") {
    return t("journal.recap.eyebrow.week", {
      start: formatDayMonth(period.start),
      end: formatDayMonth(period.end),
    });
  }
  if (period.kind === "month") {
    return t("journal.recap.eyebrow.month", {
      month: MONTH.format(dayDate(period.start)),
      year: period.start.slice(0, 4),
    });
  }
  return t("journal.recap.eyebrow.year", { year: period.start.slice(0, 4) });
}

function Empty() {
  return <p className={styles.pickerEmpty}>{t("journal.recap.nothing")}</p>;
}

function Days({ today, onPick }: { today: string; onPick: () => void }) {
  const days = useJournalDays();
  const list = [...new Set([...(days.data ?? []), today])]
    .filter((day) => day <= today)
    .sort()
    .reverse();
  if (list.length === 0) return <Empty />;
  return (
    <ul className={styles.pickerList}>
      {list.map((day) => (
        <li key={day}>
          <a
            className={styles.pickerLink}
            href={day === today ? "#/journal" : `#/journal/${day}`}
            onClick={onPick}
          >
            {formatLongDay(day)}
          </a>
        </li>
      ))}
    </ul>
  );
}

function Periods({ kind, onPick }: { kind: PeriodKind; onPick: () => void }) {
  const periods = usePeriods(kind);
  const list = periods.data ?? [];
  if (list.length === 0) return <Empty />;
  return (
    <ul className={styles.pickerList}>
      {list.map(({ period }) => (
        <li key={period.start}>
          <a
            className={styles.pickerLink}
            href={`#/journal/look-back/${kind}/${period.start}`}
            onClick={onPick}
          >
            {periodLabel(period)}
          </a>
        </li>
      ))}
    </ul>
  );
}

/** "Choose a day": a day to open, or a week, month or year to look back on. Never past today. */
export function DayPicker({ today, onClose }: { today: string; onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("day");
  return (
    <Dialog title={t("journal.day.choose")} onClose={onClose}>
      <Tabs
        label={t("journal.day.choose")}
        value={tab}
        onChange={(id) => setTab(id as Tab)}
        tabs={TABS.map((id) => ({
          id,
          label: t(LABEL[id]),
          panel:
            id === "day" ? (
              <Days today={today} onPick={onClose} />
            ) : (
              <Periods kind={id} onPick={onClose} />
            ),
        }))}
      />
    </Dialog>
  );
}
