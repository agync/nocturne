import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { hashHref, useQueryParam } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { kindName, ShowAsList, usualWaiting } from "../../components/health";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { isDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { PhoneSources, ReadingKind, ReadingRow } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { DeleteReadings } from "./DeleteReadings";
import styles from "./Health.module.css";
import { broughtKinds, healthPhones, type Range, rangeFrom, slugOf } from "./kinds";
import { type OnDelete, ReadingsList } from "./ReadingsList";
import { buildView, workoutsLine } from "./view";

const RANGES = [
  ["30", "health.range.30"],
  ["90", "health.range.90"],
  ["year", "health.range.year"],
] as const satisfies ReadonlyArray<
  readonly [Range, "health.range.30" | "health.range.90" | "health.range.year"]
>;

const HAS_SERIES: ReadonlyArray<ReadingKind> = [
  "sleep",
  "restingHeartRate",
  "weight",
  "steps",
  "bloodPressure",
];

interface KindPageProps {
  kind: ReadingKind;
  today: string;
  phones: readonly PhoneSources[];
}

export function KindPage({ kind, today, phones }: KindPageProps) {
  const engine = useEngine();
  const platform = usePlatform();
  const client = useQueryClient();
  const [range, setRange] = useState<Range>("30");
  const [deleting, setDeleting] = useState<false | { row: ReadingRow | null }>(false);
  const opener = useRef<HTMLElement | null>(null);
  const foot = useRef<HTMLSpanElement>(null);
  const returnTo = useRef<"opener" | "foot" | null>(null);
  const dayParam = useQueryParam("day");
  const phoneParam = useQueryParam("phone");
  const selected = dayParam !== null && isDay(dayParam) ? dayParam : null;

  const bringing = healthPhones(phones);
  const phone = bringing.find((p) => p.deviceId === phoneParam) ?? bringing[0];
  const tabs = phone ? broughtKinds([phone]) : [];
  const keep = { day: selected, phone: phoneParam };
  const receipt = (day: string) =>
    hashHref(`/journal/health/${slugOf(kind)}`, { day, phone: phoneParam });
  const from = rangeFrom(range, today);

  const all = useQuery({
    queryKey: ["health", "readings", kind, from, today],
    queryFn: () => engine.listReadings(kind, from, today),
  });
  const series = useQuery({
    queryKey: ["health", "series", kind, from, today],
    queryFn: () => engine.getHealthSeries(kind, from, today),
    enabled: HAS_SERIES.includes(kind),
  });

  const rows: ReadingRow[] = (all.data ?? []).filter((r) => r.day >= from);
  const shown = rows.filter((r) => !r.removed);
  const heartDay = kind === "heartRate" ? shown[0]?.day : undefined;

  const workouts = useQuery({
    queryKey: ["health", "workouts", heartDay],
    queryFn: () => engine.listReadings("workout", heartDay ?? today, heartDay ?? today),
    enabled: heartDay !== undefined,
  });

  const failed = all.isError || (HAS_SERIES.includes(kind) && series.isError);
  const loading = !failed && (all.isPending || (HAS_SERIES.includes(kind) && series.isPending));
  const view =
    loading || failed
      ? null
      : buildView({
          kind,
          long: range !== "30",
          from,
          to: today,
          today,
          readings: shown,
          series: series.data ?? null,
          workouts: (workouts.data ?? []).filter((r) => !r.removed),
          selected,
          receipt,
        });
  const workoutLine = kind === "workout" ? workoutsLine(shown) : "";

  const closeDialog = (after: "opener" | "foot") => {
    returnTo.current = after;
    setDeleting(false);
  };
  useEffect(() => {
    if (deleting || returnTo.current === null) return;
    const target =
      returnTo.current === "opener" && opener.current?.isConnected
        ? opener.current
        : foot.current?.querySelector("button");
    returnTo.current = null;
    target?.focus();
  }, [deleting]);
  const askDelete: OnDelete = (row, from) => {
    opener.current = from;
    setDeleting({ row });
  };

  return (
    <ScreenSheet>
      <div className={styles.page}>
        <div className={styles.noPrint}>
          <BackLink to="/journal" label={t("screen.journal.title")} />
        </div>
        <nav className={`${styles.tabs} ${styles.noPrint}`} aria-label={t("screen.health.title")}>
          {tabs.map((k) => (
            <a
              key={k}
              className={styles.tab}
              href={hashHref(`/journal/health/${slugOf(k)}`, keep)}
              aria-current={k === kind ? "page" : undefined}
            >
              {kindName(k)}
            </a>
          ))}
        </nav>
        <header className={styles.head}>
          <h1 className={styles.title}>{kindName(kind)}</h1>
          <fieldset
            className={`${styles.segments} ${styles.noPrint}`}
            aria-label={t("health.range.label")}
          >
            {RANGES.map(([value, copy]) => (
              <button
                key={value}
                type="button"
                className={styles.segment}
                aria-pressed={range === value}
                onClick={() => setRange(value)}
              >
                {t(copy)}
              </button>
            ))}
          </fieldset>
        </header>

        {view ? (
          <ShowAsList
            sentence={view.sentence}
            chart={view.chart}
            columns={view.columns}
            rows={view.rows}
          />
        ) : null}
        {workoutLine ? <p className={styles.sentence}>{workoutLine}</p> : null}
        {failed ? (
          <div role="alert">
            <p className={styles.sentence}>{t("health.error")}</p>
            <Button
              variant="secondary"
              onClick={() => {
                void all.refetch();
                if (HAS_SERIES.includes(kind)) void series.refetch();
              }}
            >
              {t("button.tryAgain")}
            </Button>
          </div>
        ) : null}
        {loading ? (
          <p className={styles.sentence} role="status">
            {t("health.loading")}
          </p>
        ) : null}
        {!loading && !failed && !view && !workoutLine ? (
          <p className={styles.sentence}>{t("health.empty")}</p>
        ) : null}
        {view?.waiting ? <p className={styles.sentence}>{usualWaiting()}</p> : null}
        <p className={styles.fixed}>{t("health.fixed")}</p>

        <ReadingsList rows={rows} selected={selected} onDelete={phone ? askDelete : undefined} />

        <div className={`${styles.foot} ${styles.noPrint}`}>
          <Button variant="secondary" onClick={() => void platform.print()}>
            {t("health.print")}
          </Button>
          {phone ? (
            <span ref={foot}>
              <Button
                variant="forget"
                onClick={(event) => {
                  opener.current = event.currentTarget;
                  setDeleting({ row: null });
                }}
              >
                {t("health.delete.open")}
              </Button>
            </span>
          ) : null}
        </div>
      </div>
      {deleting && phone ? (
        <DeleteReadings
          phones={bringing}
          phone={phone}
          current={kind}
          single={deleting.row}
          onClose={() => closeDialog("opener")}
          onDeleted={() => {
            closeDialog(deleting.row ? "foot" : "opener");
            void client.invalidateQueries();
          }}
        />
      ) : null}
    </ScreenSheet>
  );
}
