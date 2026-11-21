import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { readingDay } from "../../components/ReceiptChip/receiptPath";
import { t } from "../../copy/t";
import type { FeelingPart, PeriodKind, Receipt, SameNameQuestion } from "../../engine/bindings";
import type { Connection, PairedDevice } from "../engine";
import { addressFromRelinkText, addressFromWords } from "../relink";
import { Ask, useAskSession } from "../screens/Ask";
import { CheckIn } from "../screens/CheckIn";
import { RelinkWords } from "../screens/Entry";
import { Removed, TurnedOff } from "../screens/Home";
import { Journal } from "../screens/Journal";
import { Keep } from "../screens/Keep";
import { LookBack } from "../screens/LookBack";
import { Original } from "../screens/Original";
import { Which } from "../screens/Which";
import type { NativeChannel } from "./api";
import { type LaunchRoute, type Shared, watchLaunch } from "./launch";
import { scanCode } from "./scanner";
import { CalendarConnect } from "./screens/CalendarConnect";
import { KeepButton, NavBar, type Tab, TabBar } from "./screens/chrome";
import { HealthConnect } from "./screens/HealthConnect";
import { Offer } from "./screens/Offer";
import { queueShared, ShareSheet } from "./screens/ShareSheet";
import { ThisPhone } from "./screens/ThisPhone";
import { Today } from "./screens/Today";
import type { UpdateNeeded } from "./screens/UpdateCard";
import type { NativeServices } from "./services";

type View =
  | { name: "tab"; tab: Tab }
  | { name: "offer" }
  | { name: "thisPhone" }
  | { name: "calendar" }
  | { name: "health" }
  | { name: "keep"; recording?: boolean }
  | { name: "checkin"; part?: FeelingPart }
  | { name: "original"; id: string }
  | { name: "journal"; day: string; today: string | null }
  | {
      name: "lookback";
      kind: PeriodKind;
      start: string;
      end: string;
      write: boolean;
      today: string | null;
    }
  | { name: "which"; id: string; day: string; today: string | null }
  | { name: "relinkWords"; busy: boolean; error: boolean };

const TODAY: View = { name: "tab", tab: "today" };

/** Which side must update, from the connection, with both versions for the card. */
export function updateFor(
  conn: Connection,
  device: PairedDevice,
  phone: string,
  ios = false,
): UpdateNeeded | null {
  const known = device.version ?? "";
  if (conn.kind === "outdated")
    return { kind: "phone", computer: conn.computer ?? known, phone, ios };
  if (conn.kind === "computerOutdated")
    return { kind: "computer", computer: conn.computer || known, phone, ios };
  return null;
}
const RETRY_MS = 30_000;

/** Views live in the history entries, so the phone's back gesture steps back through the app. */
function useView(first: View) {
  const [view, setView] = useState<View>(() => {
    // Just paired: the offer comes first, whatever an earlier screen left in history.
    if (first.name === "offer") return first;
    const kept = history.state?.view as View | { name: "journal"; day: null } | undefined;
    if (kept?.name === "journal" && !kept.day) return { name: "tab", tab: "journal" };
    return (kept as View | undefined) ?? first;
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

interface NativeAppProps {
  services: NativeServices;
  channel: NativeChannel;
  initial: PairedDevice;
  /** Just paired: "Bring more from this phone?" comes first. */
  offer: boolean;
  /** Which side needs an update, once the version window says so. */
  update?: UpdateNeeded | null;
  onUnpaired(unreached: boolean): void;
}

/**
 * The paired app: Today, Journal and Ask in the bar at the bottom, Keep floating over Today and
 * Journal, and This phone in the top bar.
 */
export function NativeApp({
  services,
  channel,
  initial,
  offer,
  update = null,
  onUnpaired,
}: NativeAppProps) {
  const [device, setDevice] = useState(initial);
  const address = useRef(initial.address);
  address.current = device.address;
  const engine = useMemo(() => services.engineFor(() => address.current), [services]);
  useEffect(() => () => engine.close(), [engine]);
  const conn = useSyncExternalStore(engine.subscribe, engine.connection);
  const removed = useRef(false);
  removed.current = conn.kind === "removed";
  const { view, go, back, replace } = useView(offer ? { name: "offer" } : TODAY);
  const [tab, setTab] = useState<Tab>(view.name === "tab" ? view.tab : "today");
  const [waiting, setWaiting] = useState(0);
  const [sent, setSent] = useState(0);
  const [shared, setShared] = useState<Shared | null>(null);
  const [chosen, setChosen] = useState<ReadonlySet<"calendar" | "health">>(new Set());
  const ask = useAskSession(engine);
  const quote = useRef<string | null>(null);
  const question = useRef<SameNameQuestion | null>(null);
  const draft = useRef("");

  useEffect(() => {
    if (view.name === "tab") setTab(view.tab);
  }, [view]);

  const refreshWaiting = useCallback(() => {
    services.queue.count().then(setWaiting, () => undefined);
  }, [services.queue]);
  const refreshDevice = useCallback(() => {
    services.store.device().then(
      (d) => d && setDevice(d),
      () => undefined,
    );
  }, [services.store]);
  useEffect(refreshWaiting, [refreshWaiting]);

  useEffect(() => {
    void engine.connect();
    const poke = () => {
      if (document.visibilityState === "visible") void engine.connect();
    };
    window.addEventListener("online", poke);
    document.addEventListener("visibilitychange", poke);
    return () => {
      window.removeEventListener("online", poke);
      document.removeEventListener("visibilitychange", poke);
    };
  }, [engine]);
  useEffect(() => {
    // One side must update: trying again on a timer changes nothing. Coming back to the app or
    // to the network still tries.
    const settled = ["connected", "removed", "outdated", "computerOutdated"];
    if (settled.includes(conn.kind)) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void engine.connect();
    }, RETRY_MS);
    return () => clearInterval(timer);
  }, [conn.kind, engine]);

  const connectedAt = conn.kind === "connected" ? conn.hello : null;
  useEffect(() => {
    if (!connectedAt) return;
    refreshDevice();
    let gone = false;
    services.queue.flush(engine).then((result) => {
      if (gone) return;
      if (result.sent > 0) setSent(result.sent);
      setWaiting(result.remaining);
    });
    return () => {
      gone = true;
    };
  }, [connectedAt, engine, services.queue, refreshDevice]);

  // A shortcut on the icon opens its screen; a share opens the sheet over whatever is open.
  useEffect(
    () =>
      watchLaunch(
        (route: LaunchRoute) => {
          if (route === "keep") go({ name: "keep" });
          else if (route === "record") go({ name: "keep", recording: true });
          else go({ name: "checkin" });
        },
        (next) => {
          if (removed.current)
            void queueShared(next, services.queue, services.now).then(refreshWaiting);
          else setShared(next);
        },
      ),
    [go, services, refreshWaiting],
  );

  const open = useCallback(
    (receipt: Receipt) => {
      // A reading has no page of its own: it opens the day of the journal it belongs to.
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
  const clearWaiting = useCallback(async () => {
    await services.queue.clear();
    setWaiting(0);
  }, [services.queue]);

  const relinked = async (result: { ok: boolean }) => {
    if (!result.ok) return replace({ name: "relinkWords", busy: false, error: true });
    refreshDevice();
    void engine.connect();
    replace(TODAY);
  };
  const relink = async (kind: "scan" | "words") => {
    if (kind === "words") return go({ name: "relinkWords", busy: false, error: false });
    const got = await scanCode();
    if (got.kind === "cancelled") return;
    if (got.kind === "cameraOff") return go({ name: "relinkWords", busy: false, error: false });
    const address = addressFromRelinkText(got.text);
    if (!address) return go({ name: "relinkWords", busy: false, error: true });
    await relinked(await services.relink(address));
  };

  const toTab = (next: Tab) => {
    if (view.name === "tab") replace({ name: "tab", tab: next });
    else go({ name: "tab", tab: next });
  };
  const thisPhone = () => go({ name: "thisPhone" });
  const bar = (forTab: Tab, action?: ReactNode) => (
    <TabBar tab={forTab} action={action} onThisPhone={thisPhone} />
  );
  const toKeep = () => go({ name: "keep" });
  const computerName =
    conn.kind === "connected"
      ? conn.hello.computerName
      : (device.computerName ?? t("phone.home.computer"));
  const sheet = shared ? (
    <ShareSheet
      shared={shared}
      engine={engine}
      queue={services.queue}
      connection={conn}
      now={services.now}
      onQueued={refreshWaiting}
      onClose={() => setShared(null)}
    />
  ) : null;

  if (conn.kind === "removed") return <Removed waiting={waiting} onClear={clearWaiting} />;

  let screen: ReactNode = null;
  let tabbed = false;
  switch (view.name) {
    case "offer":
      screen = (
        <Offer
          channel={channel}
          connected={conn.kind === "connected"}
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
      screen = <CalendarConnect channel={channel} onDone={back} />;
      break;
    case "health":
      screen = <HealthConnect channel={channel} onDone={back} />;
      break;
    case "thisPhone":
      screen = (
        <ThisPhone
          channel={channel}
          device={device}
          connection={conn}
          version={services.version}
          waiting={waiting}
          onBack={back}
          onCalendar={() => go({ name: "calendar" })}
          onHealth={() => go({ name: "health" })}
          onDisconnect={async () => {
            const { reached } = await engine.disconnect();
            onUnpaired(!reached);
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
          deviceName={device.deviceName}
          now={services.now}
          onHome={back}
          onQueued={refreshWaiting}
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
          deviceName={device.deviceName}
          now={services.now}
          part={view.part}
          onBack={back}
          onQueued={refreshWaiting}
          lastNight
        />
      );
      break;
    case "original":
      screen = <Original engine={engine} itemId={view.id} quote={quote.current} onBack={back} />;
      break;
    case "lookback":
      screen = (
        <LookBack
          engine={engine}
          period={{ kind: view.kind, start: view.start, end: view.end }}
          write={view.write}
          today={view.today}
          now={services.now}
          onStarted={() => replace({ ...view, write: false })}
          onBack={back}
          onToday={() => toTab("journal")}
          onItem={openItem}
          onReceipt={open}
        />
      );
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
    case "relinkWords":
      screen = (
        <RelinkWords
          busy={view.busy}
          error={view.error}
          onBack={back}
          onScan={() => void relink("scan")}
          onSubmit={async (words) => {
            const address = addressFromWords(words);
            if (!address) return replace({ name: "relinkWords", busy: false, error: true });
            replace({ name: "relinkWords", busy: true, error: false });
            await relinked(await services.relink(address));
          }}
        />
      );
      break;
    case "journal":
    case "tab": {
      const showing: Tab | "day" = view.name === "journal" ? "day" : view.tab;
      const journalProps = {
        engine,
        queue: services.queue,
        connection: conn,
        computerName,
        onDay: (day: string, today: string | null) => go({ name: "journal", day, today }),
        onOpen: openItem,
        onWhich: (q: SameNameQuestion, day: string, today: string | null) => {
          question.current = q;
          go({ name: "which", id: q.id, day, today });
        },
        onReflect: (day: string, today: string | null) =>
          go({ name: "lookback", kind: "day", start: day, end: day, write: true, today }),
        onReflection: (day: string, today: string | null) =>
          go({ name: "lookback", kind: "day", start: day, end: day, write: false, today }),
        onKeep: toKeep,
        onCheckIn: () => go({ name: "checkin" }),
        onBack: back,
        phone: true,
      };
      if (showing === "day" && view.name === "journal") {
        screen = <Journal {...journalProps} day={view.day} today={view.today} />;
        break;
      }
      if (conn.kind === "turnedOff") {
        screen = (
          <TurnedOff
            device={device}
            waiting={waiting}
            now={services.now()}
            onKeep={toKeep}
            onMenu={thisPhone}
          />
        );
        break;
      }
      tabbed = true;
      if (showing === "today") {
        screen = (
          <Today
            engine={engine}
            queue={services.queue}
            connection={conn}
            device={device}
            waiting={waiting}
            sentJustNow={sent}
            now={services.now()}
            update={update ?? updateFor(conn, device, services.version, services.platform.ios)}
            bar={bar("today")}
            onQueued={refreshWaiting}
            onOpen={open}
            onJournal={(day) =>
              day ? go({ name: "journal", day, today: null }) : toTab("journal")
            }
            onRelink={(kind) => void relink(kind)}
          />
        );
      } else if (showing === "journal") {
        screen = (
          <Journal
            {...journalProps}
            day={null}
            today={null}
            tabBar={(action) => bar("journal", action)}
          />
        );
      } else {
        screen = (
          <Ask
            model={ask.model}
            connected={conn.kind === "connected"}
            onAsk={(q) => void ask.ask(q)}
            onStop={() => void ask.stop()}
            onOpen={open}
            onBack={back}
            onSeeChange={() => void ask.seeChange()}
            onAskHere={() => void ask.askHere()}
            tabBar={bar("ask")}
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
      {sheet}
    </>
  );
}
