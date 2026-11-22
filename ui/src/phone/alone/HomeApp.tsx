import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { readingDay } from "../../components/ReceiptChip/receiptPath";
import { localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { FeelingPart, HomeStats, Receipt, SameNameQuestion } from "../../engine/bindings";
import type { NativeChannel } from "../native/api";
import { type LaunchRoute, type Shared, watchLaunch } from "../native/launch";
import { scanCode } from "../native/scanner";
import { CalendarConnect } from "../native/screens/CalendarConnect";
import { KeepButton, NavBar, type Tab, TabBar } from "../native/screens/chrome";
import { HealthConnect } from "../native/screens/HealthConnect";
import { Offer } from "../native/screens/Offer";
import { ShareSheet } from "../native/screens/ShareSheet";
import type { NativeServices } from "../native/services";
import { CheckIn } from "../screens/CheckIn";
import { Journal } from "../screens/Journal";
import { Keep } from "../screens/Keep";
import { Original } from "../screens/Original";
import { Which } from "../screens/Which";
import "./home.css";
import { reminderDone, reminderDue } from "./reminder";
import { SaveBackup } from "./screens/Backup";
import { DeleteEverything } from "./screens/DeleteEverything";
import { HomeFind } from "./screens/Find";
import { HomeThisPhone } from "./screens/ThisPhone";
import { HomeToday } from "./screens/Today";
import { GiveMind } from "./screens/Welcome";

type View =
  | { name: "tab"; tab: Tab }
  | { name: "offer" }
  | { name: "thisPhone" }
  | { name: "calendar" }
  | { name: "health" }
  | { name: "mind" }
  | { name: "backup" }
  | { name: "delete" }
  | { name: "keep"; recording?: boolean }
  | { name: "checkin"; part?: FeelingPart }
  | { name: "original"; id: string }
  | { name: "journal"; day: string; today: string | null }
  | { name: "which"; id: string; day: string; today: string | null };

const TODAY: View = { name: "tab", tab: "today" };

/** Views live in the history entries, so the phone's back gesture steps back through the app. */
function useView(first: View) {
  const [view, setView] = useState<View>(() => {
    if (first.name === "offer") return first;
    return (history.state?.view as View | undefined) ?? first;
  });
  useEffect(() => {
    history.replaceState({ view }, "");
    const onPop = (e: PopStateEvent) => setView((e.state?.view as View | undefined) ?? TODAY);
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
    // The first entry is recorded once.
  }, []);
  const go = useCallback((next: View) => {
    history.pushState({ view: next }, "");
    setView(next);
  }, []);
  const replace = useCallback((next: View) => {
    history.replaceState({ view: next }, "");
    setView(next);
  }, []);
  const back = useCallback(() => {
    if (history.length > 1 && history.state?.view?.name !== "tab") history.back();
    else replace(TODAY);
  }, [replace]);
  return { view, go, back, replace };
}

/** The phone's language for reading voice notes, as the web view knows it. */
const language = () => navigator.language || "en-GB";

/**
 * A phone alone: Today, Journal and Ask from this phone's own memory, Keep floating over Today
 * and Journal, and This phone in the top bar, with its backup and Delete everything.
 */
export function HomeApp({
  services,
  channel,
  offer,
  name,
}: {
  services: NativeServices;
  channel: NativeChannel;
  /** Just started: "Bring more from this phone?" comes first. */
  offer: boolean;
  /** This phone's name. */
  name: string;
}) {
  const { home } = services;
  const engine = useMemo(() => services.engineFor(() => ""), [services]);
  useEffect(() => () => engine.close(), [engine]);
  const conn = engine.connection();
  const { view, go, back, replace } = useView(offer ? { name: "offer" } : TODAY);
  const [tab, setTab] = useState<Tab>(view.name === "tab" ? view.tab : "today");
  const [chosen, setChosen] = useState<ReadonlySet<"calendar" | "health">>(new Set());
  const [shared, setShared] = useState<Shared | null>(null);
  const [empty, setEmpty] = useState(false);
  const [fresh, setFresh] = useState(0);
  const question = useRef<SameNameQuestion | null>(null);
  const quote = useRef<string | null>(null);
  const draft = useRef("");

  useEffect(() => {
    if (view.name === "tab") setTab(view.tab);
  }, [view]);

  const stats = useCallback(() => home.stats(), [home]);
  const due = useCallback(
    (s: HomeStats) => reminderDue(s, services.now(), home.notes),
    [services, home],
  );
  const calendarOn = useCallback(
    async () => (await channel.sources()).calendar === "on",
    [channel],
  );
  const refresh = useCallback(() => setFresh((n) => n + 1), []);

  // What waited in the keep queue (the store was busy) goes into the memory now.
  useEffect(() => {
    void services.queue.flush(engine).then(
      (r) => r.sent > 0 && refresh(),
      () => undefined,
    );
  }, [engine, services.queue, refresh]);

  // Voice notes are read into words where the phone can: on open and each time it comes back.
  useEffect(() => {
    void home.hearVoiceNotes(language());
    const front = () => {
      if (document.visibilityState === "visible") void home.hearVoiceNotes(language());
    };
    document.addEventListener("visibilitychange", front);
    return () => document.removeEventListener("visibilitychange", front);
  }, [home]);

  useEffect(
    () =>
      watchLaunch(
        (route: LaunchRoute) => {
          if (route === "keep") go({ name: "keep" });
          else if (route === "record") go({ name: "keep", recording: true });
          else go({ name: "checkin" });
        },
        (next) => setShared(next),
      ),
    [go],
  );

  useEffect(() => {
    if (tab !== "ask") return;
    let gone = false;
    home.stats().then(
      (s) => !gone && setEmpty(s.counts.items === 0),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [tab, home]);

  const open = useCallback(
    (receipt: Receipt) => {
      if (receipt.kind === "reading")
        return go({ name: "journal", day: readingDay(receipt.keptAt), today: null });
      quote.current = receipt.span?.type === "quote" ? receipt.span.text : null;
      go({ name: "original", id: receipt.itemId });
    },
    [go],
  );
  const openItem = useCallback(
    (id: string) => {
      quote.current = null;
      go({ name: "original", id });
    },
    [go],
  );

  const toTab = (next: Tab) => {
    if (view.name === "tab") replace({ name: "tab", tab: next });
    else go({ name: "tab", tab: next });
  };
  const thisPhone = () => go({ name: "thisPhone" });
  const bar = (forTab: Tab, action?: ReactNode) => (
    <TabBar tab={forTab} action={action} onThisPhone={thisPhone} />
  );
  const toKeep = () => go({ name: "keep" });
  const toMind = () => go({ name: "mind" });

  let screen: ReactNode = null;
  let tabbed = false;
  switch (view.name) {
    case "offer":
      screen = (
        <Offer
          channel={channel}
          connected
          alone
          chosen={chosen}
          onCalendar={() => {
            setChosen((s) => new Set(s).add("calendar"));
            go({ name: "calendar" });
          }}
          onHealth={() => {
            setChosen((s) => new Set(s).add("health"));
            go({ name: "health" });
          }}
          onDone={() => replace(TODAY)}
        />
      );
      break;
    case "calendar":
      screen = <CalendarConnect channel={channel} onDone={back} alone />;
      break;
    case "health":
      screen = <HealthConnect channel={channel} onDone={back} />;
      break;
    case "thisPhone":
      screen = (
        <HomeThisPhone
          channel={channel}
          stats={stats}
          name={name}
          onBack={back}
          onMind={toMind}
          onCalendar={() => go({ name: "calendar" })}
          onHealth={() => go({ name: "health" })}
          onBackup={() => go({ name: "backup" })}
          onDelete={() => go({ name: "delete" })}
        />
      );
      break;
    case "mind":
      screen = (
        <GiveMind
          backLabel={t("phone.back")}
          onBack={back}
          onScan={() => void scanCode().catch(() => undefined)}
        />
      );
      break;
    case "backup":
      screen = (
        <SaveBackup
          save={home.saveBackup}
          pickDest={home.files.saveTo}
          now={services.now}
          onClose={back}
        />
      );
      break;
    case "delete":
      screen = (
        <DeleteEverything
          onKeep={back}
          onDelete={async (phrase) => {
            await home.deleteEverything(phrase);
            refresh();
            replace(TODAY);
          }}
        />
      );
      break;
    case "keep":
      screen = (
        <Keep
          engine={engine}
          queue={services.queue}
          connection={conn}
          deviceName={name}
          now={services.now}
          onHome={back}
          onQueued={refresh}
          onMoreWords={(part) => go({ name: "checkin", part })}
          draft={draft}
          recording={view.recording}
        />
      );
      break;
    case "checkin":
      screen = (
        <CheckIn
          engine={engine}
          queue={services.queue}
          connection={conn}
          deviceName={name}
          now={services.now}
          part={view.part}
          onBack={back}
          onQueued={refresh}
          lastNight
        />
      );
      break;
    case "original":
      screen = <Original engine={engine} itemId={view.id} quote={quote.current} onBack={back} />;
      break;
    case "which":
      screen = (
        <Which
          engine={engine}
          question={question.current?.id === view.id ? question.current : null}
          questionId={view.id}
          day={view.day}
          today={view.day === view.today}
          onBack={back}
          onDone={back}
        />
      );
      break;
    case "journal":
    case "tab": {
      const journalProps = {
        engine,
        queue: services.queue,
        connection: conn,
        computerName: name,
        onDay: (day: string, today: string | null) => go({ name: "journal", day, today }),
        onOpen: openItem,
        onWhich: (q: SameNameQuestion, day: string, today: string | null) => {
          question.current = q;
          go({ name: "which", id: q.id, day, today });
        },
        onReflect: () => undefined,
        onReflection: () => undefined,
        onKeep: toKeep,
        onCheckIn: () => go({ name: "checkin" }),
        onBack: back,
        phone: true,
        noMind: true,
      };
      if (view.name === "journal") {
        screen = <Journal {...journalProps} day={view.day} today={view.today} />;
        break;
      }
      tabbed = true;
      if (view.tab === "today") {
        screen = (
          <HomeToday
            key={fresh}
            engine={engine}
            bar={bar("today")}
            calendarOn={calendarOn}
            stats={stats}
            reminderDue={due}
            onReminder={(save) => {
              void reminderDone(home.notes);
              if (save) go({ name: "backup" });
            }}
            onMind={toMind}
            onCheckIn={() => go({ name: "checkin" })}
            onItem={openItem}
            onCalendarItem={openItem}
            onNight={(to) => go({ name: "journal", day: readingDay(to), today: null })}
            onJournal={() => toTab("journal")}
          />
        );
      } else if (view.tab === "journal") {
        screen = (
          <Journal
            key={fresh}
            {...journalProps}
            day={null}
            today={null}
            tabBar={(action) => bar("journal", action)}
          />
        );
      } else {
        screen = (
          <HomeFind
            bar={bar("ask")}
            empty={empty}
            today={localDay(new Date(services.now()))}
            find={home.find}
            onOpen={open}
            onMind={toMind}
          />
        );
      }
      break;
    }
  }

  return (
    <>
      {screen}
      {tabbed ? (
        <>
          {tab !== "ask" ? <KeepButton onKeep={toKeep} /> : null}
          <NavBar tab={tab} onTab={toTab} />
        </>
      ) : null}
      {shared ? (
        <ShareSheet
          shared={shared}
          engine={engine}
          queue={services.queue}
          connection={conn}
          now={services.now}
          onQueued={refresh}
          onClose={() => {
            setShared(null);
            refresh();
          }}
        />
      ) : null}
    </>
  );
}
