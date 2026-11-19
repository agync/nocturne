import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useRoute } from "wouter";
import { hashHref, useQueryParam } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type {
  Glance,
  GlanceReason,
  JournalDay,
  Period,
  ReflectionView,
} from "../../engine/bindings";
import { useEngine, useJournalDay, useReflection, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { GlanceView } from "./Glance";
import styles from "./Lookback.module.css";
import { lookbackPath, periodFor } from "./period";
import { periodText, RecapView, recapEyebrow, recapTitle } from "./Recap";
import { clock } from "./text";
import { dayEyebrow, dayTitle, WrittenView } from "./Written";

interface ViewProps {
  period: Period;
  today: string | undefined;
  view: ReflectionView | { type: "failed" };
  /** What to show while a look back is being written here, or null. */
  writing: Glance | null | "none";
  onWrite: () => void;
  /** Wait for what is still being read, then write. */
  onWriteWhenRead: () => void;
  waiting: boolean;
  onStop: () => void;
  onPrint: () => void;
  onDelete: () => void;
}

const READ_POLL_MS = 3000;

const beingRead = (day: JournalDay) =>
  [...day.entry.map((b) => b.item), ...day.rows.map((r) => r.item)].some(
    (item) => item.status === "kept" || item.status === "reading",
  );

const onDemandId = (kind: Period["kind"]) =>
  kind === "week"
    ? "journal.recap.onDemand.week"
    : kind === "month"
      ? "journal.recap.onDemand.month"
      : "journal.recap.onDemand.year";

function Header({ eyebrow, title }: { eyebrow: string; title: string }) {
  return (
    <header className={styles.head}>
      <p className={styles.eyebrow}>{eyebrow}</p>
      <h1 className={`t-page-title ${styles.title}`}>{title}</h1>
    </header>
  );
}

function Writing({ glance, onStop }: { glance: Glance | null; onStop: () => void }) {
  return (
    <>
      <div className={styles.writing} role="status">
        <span className={styles.pulse} aria-hidden="true" />
        <span>
          <strong>{t("journal.reflection.writing.title")}</strong>{" "}
          {t("journal.reflection.writing.body")}
        </span>
        <Button variant="quiet" onClick={onStop}>
          {t("journal.reflection.writing.stop")}
        </Button>
      </div>
      {glance ? <GlanceView glance={glance} /> : null}
    </>
  );
}

function glanceLead(reason: GlanceReason): string | null {
  if (reason === "notReady") return t("journal.reflection.glance.notReady");
  if (reason === "notOnThisComputer") return t("journal.reflection.glance.notOnThisComputer");
  return null;
}

function Body(props: ViewProps) {
  const { period, view, today } = props;
  const isDay = period.kind === "day";

  if (props.writing !== "none") {
    return <Writing glance={props.writing} onStop={props.onStop} />;
  }
  switch (view.type) {
    case "written":
      return (
        <WrittenView
          reflection={view.reflection}
          onWriteAgain={props.onWrite}
          onWriteWhenRead={props.onWriteWhenRead}
          waiting={props.waiting}
          onPrint={props.onPrint}
          onDelete={props.onDelete}
        />
      );
    case "recap":
      return <RecapView recap={view.recap} onWriteAgain={props.onWrite} onPrint={props.onPrint} />;
    case "writing":
      return <Writing glance={view.glance} onStop={props.onStop} />;
    case "glance": {
      const lead = glanceLead(view.reason);
      return (
        <>
          {lead ? <p className={styles.lead}>{lead}</p> : null}
          <GlanceView glance={view.glance} />
        </>
      );
    }
    case "notYet":
      return (
        <>
          <GlanceView glance={view.glance} />
          <div className={styles.buttons}>
            <Button variant="primary" onClick={props.onWrite}>
              {isDay
                ? t(period.start === today ? "journal.day.reflect" : "journal.day.reflectPast")
                : t(onDemandId(period.kind))}
            </Button>
          </div>
        </>
      );
    case "nothing":
      return (
        <p className={styles.prose}>
          {isDay
            ? t("journal.reflection.nothing")
            : t(`journal.recap.nothing.${period.kind as "week" | "month" | "year"}`)}
        </p>
      );
    case "failed":
      return (
        <div className={styles.notice} role="alert">
          <span>{t("journal.reflection.failed")}</span>
          <Button variant="secondary" onClick={props.onWrite}>
            {t("button.tryAgain")}
          </Button>
        </div>
      );
  }
}

function headerOf(period: Period, view: ViewProps["view"], today: string | undefined) {
  if (view.type === "written") {
    return { eyebrow: dayEyebrow(view.reflection), title: dayTitle(period.start) };
  }
  if (period.kind === "day") {
    const eyebrow =
      period.start === today
        ? t("journal.reflection.eyebrow.soFar", { time: clock(new Date()) })
        : t("journal.day.eyebrow");
    return { eyebrow, title: dayTitle(period.start) };
  }
  const kind = period.kind;
  return {
    eyebrow: view.type === "recap" ? recapEyebrow(view.recap) : periodText(period),
    title: recapTitle(kind),
  };
}

function Loaded({ period }: { period: Period }) {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const refresh = useRefresh();
  const client = useQueryClient();
  const query = useReflection(period);
  const todayQuery = useJournalDay();
  const write = useQueryParam("write") === "1";
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const started = useRef(false);
  const mounted = useRef(true);
  const wake = useRef<(() => void) | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      wake.current?.();
    };
  }, []);

  const glanceNow = (): Glance | null => {
    const data = query.data;
    return data && (data.type === "glance" || data.type === "notYet" || data.type === "writing")
      ? data.glance
      : null;
  };

  const run = useCallback(async () => {
    setBusy(true);
    setFailed(false);
    try {
      await engine.reflect(period);
    } catch {
      if (mounted.current) setFailed(true);
    } finally {
      if (mounted.current) setBusy(false);
      refresh("journal");
    }
  }, [engine, period, refresh]);

  const writeWhenRead = async () => {
    if (waiting) return;
    setWaiting(true);
    try {
      while (mounted.current) {
        const day = await engine.getJournalDay(period.start).catch(() => null);
        if (day && !beingRead(day)) break;
        await new Promise<void>((resolve) => {
          const timer = window.setTimeout(resolve, READ_POLL_MS);
          wake.current = () => {
            window.clearTimeout(timer);
            resolve();
          };
        });
      }
      if (mounted.current) await run();
    } finally {
      if (mounted.current) setWaiting(false);
    }
  };

  useEffect(() => {
    if (!write || started.current) return;
    started.current = true;
    void run().then(() => {
      if (mounted.current) window.location.replace(hashHref(lookbackPath(period)));
    });
  }, [write, run, period]);

  const view = query.data;
  const engineWriting = view?.type === "writing";
  useEffect(() => {
    if (!engineWriting) return;
    const timer = window.setInterval(() => refresh("journal"), 4000);
    return () => window.clearInterval(timer);
  }, [engineWriting, refresh]);

  const stop = async () => {
    try {
      await engine.stopReflecting(period);
    } finally {
      refresh("journal");
    }
  };

  const remove = async () => {
    await engine.deleteReflection(period);
    await client.invalidateQueries({ queryKey: ["journal"] });
    toast.show({ message: t("journal.reflection.deleted") });
  };

  const today = todayQuery.data?.day;
  const shown: ViewProps["view"] | null =
    failed || query.isError ? { type: "failed" } : (view ?? null);
  const back =
    period.kind === "day"
      ? { to: `/journal/${period.start}`, label: t("journal.day.eyebrow") }
      : { to: "/journal", label: t("journal.day.eyebrow") };

  return (
    <ScreenSheet>
      <div className={styles.page}>
        <div className={styles.noPrint}>
          <BackLink to={back.to} label={back.label} />
        </div>
        <article className={styles.article} aria-busy={busy || query.isPending}>
          {shown ? (
            <>
              <Header {...headerOf(period, shown, today)} />
              <Body
                period={period}
                today={today}
                view={shown}
                writing={busy ? glanceNow() : "none"}
                onWrite={() => {
                  if (query.isError && !failed) void query.refetch();
                  else void run();
                }}
                onWriteWhenRead={() => void writeWhenRead()}
                waiting={waiting}
                onStop={() => void stop()}
                onPrint={() => void platform.print()}
                onDelete={() => void remove()}
              />
            </>
          ) : null}
        </article>
      </div>
    </ScreenSheet>
  );
}

/** A day's reflection or a week, month or year looked back on, at `#/journal/look-back/:kind/:start`. */
export default function Lookback() {
  const [, params] = useRoute("/journal/look-back/:kind/:start");
  const period = periodFor(params?.kind, params?.start);
  const key = period ? `${period.kind}:${period.start}` : "none";
  if (!period) {
    return (
      <ScreenSheet>
        <div className={styles.page}>
          <BackLink to="/journal" label={t("journal.day.eyebrow")} />
          <p className={styles.prose}>{t("journal.recap.nothing")}</p>
        </div>
      </ScreenSheet>
    );
  }
  return <Loaded key={key} period={period} />;
}
