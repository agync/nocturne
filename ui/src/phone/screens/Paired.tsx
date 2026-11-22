import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { t } from "../../copy/t";
import type { FeelingPart, PeriodKind, Receipt, SameNameQuestion } from "../../engine/bindings";
import type { PairedDevice } from "../engine";
import type { Services } from "../services";
import { refreshShellOnce } from "../shell";
import { Ask, useAskSession } from "./Ask";
import { CheckIn } from "./CheckIn";
import { Home, Outdated, Removed, Sheet, TurnedOff } from "./Home";
import { Journal } from "./Journal";
import { Keep } from "./Keep";
import { LookBack } from "./LookBack";
import { Original } from "./Original";
import { PageScreen } from "./PageScreen";
import { Which } from "./Which";

type View =
  | { name: "home" }
  | { name: "keep" }
  | { name: "ask" }
  | { name: "page" }
  | { name: "original"; id: string }
  | { name: "journal"; day: string | null; today: string | null }
  | { name: "checkin"; part?: FeelingPart }
  | {
      name: "lookback";
      kind: PeriodKind;
      start: string;
      end: string;
      write: boolean;
      today: string | null;
    }
  | { name: "which"; id: string; day: string; today: string | null };

const HOME: View = { name: "home" };

/** Views live in the history entries, so the phone's back gesture steps back through the app. */
function useView() {
  const [view, setView] = useState<View>(() => (history.state?.view as View | undefined) ?? HOME);
  useEffect(() => {
    history.replaceState({ view }, "");
    const onPop = (e: PopStateEvent) => setView((e.state?.view as View | undefined) ?? HOME);
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
    // The first entry is recorded once.
  }, []);
  const go = useCallback((next: View) => {
    history.pushState({ view: next }, "");
    setView(next);
  }, []);
  /** Swaps the current entry for another, so a mark that has been used does not come back. */
  const replace = useCallback((next: View) => {
    history.replaceState({ view: next }, "");
    setView(next);
  }, []);
  const back = useCallback(() => {
    if (history.length > 1 && history.state?.view?.name !== "home") history.back();
    else {
      history.replaceState({ view: HOME }, "");
      setView(HOME);
    }
  }, []);
  return { view, go, back, replace };
}

const RETRY_MS = 30_000;

interface PairedProps {
  services: Services;
  initial: PairedDevice;
  /** `unreached` when the computer could not be told, so it may still list this phone. */
  onUnpaired(unreached: boolean): void;
  onRelink(kind: "scan" | "words" | "pair" | "type"): void;
  /** Bump to re-read the device after a re-link. */
  deviceVersion: number;
}

export function Paired({ services, initial, onUnpaired, onRelink, deviceVersion }: PairedProps) {
  const [device, setDevice] = useState(initial);
  const address = useRef(initial.address);
  address.current = device.address;
  const engine = useMemo(() => services.engineFor(() => address.current), [services]);
  useEffect(() => () => engine.close(), [engine]);
  const conn = useSyncExternalStore(engine.subscribe, engine.connection);
  const { view, go, back, replace } = useView();
  const [sheet, setSheet] = useState(false);
  const [waiting, setWaiting] = useState(0);
  const [sent, setSent] = useState(0);
  const [headline, setHeadline] = useState<string | null>(null);
  const [noPage, setNoPage] = useState(false);
  const ask = useAskSession(engine);
  const quote = useRef<string | null>(null);
  const question = useRef<SameNameQuestion | null>(null);
  const draft = useRef("");
  const now = services.now();

  const refreshWaiting = useCallback(() => {
    services.queue.count().then(setWaiting, () => undefined);
  }, [services.queue]);
  const refreshDevice = useCallback(() => {
    services.store.device().then(
      (d) => d && setDevice(d),
      () => undefined,
    );
  }, [services.store]);

  // A re-link stored a new address: read it.
  useEffect(refreshDevice, [deviceVersion, refreshDevice]);
  useEffect(refreshWaiting, [refreshWaiting]);

  // Connect now, when the phone comes back online or to the foreground, and every 30 seconds
  // while it is visible and not connected.
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
    if (conn.kind === "connected" || conn.kind === "removed") return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void engine.connect();
    }, RETRY_MS);
    return () => clearInterval(timer);
  }, [conn.kind, engine]);

  // A computer that wants a newer phone app: fetch the new shell once; if it still wants more, say so.
  const minBuild = conn.kind === "outdated" ? conn.minBuild : null;
  useEffect(() => {
    if (minBuild !== null && !services.scripted) void refreshShellOnce(`build-${minBuild}`);
  }, [minBuild, services.scripted]);

  // Once connected: note the device, send what waited, read the headline, check the version.
  const connectedAt = conn.kind === "connected" ? conn.hello : null;
  useEffect(() => {
    if (!connectedAt) return;
    refreshDevice();
    if (connectedAt.version !== services.version && !services.scripted)
      void refreshShellOnce(connectedAt.version);
    let gone = false;
    services.queue.flush(engine).then((result) => {
      if (gone) return;
      if (result.sent > 0) setSent(result.sent);
      setWaiting(result.remaining);
    });
    engine.getPage().then(
      (page) => {
        if (gone) return;
        setHeadline(page.headline);
        setNoPage(false);
      },
      () => {
        if (!gone) setNoPage(true);
      },
    );
    return () => {
      gone = true;
    };
  }, [connectedAt, engine, services, refreshDevice]);

  const open = useCallback(
    (receipt: Receipt) => {
      // The words of the answer stay in memory; history entries hold no content.
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

  if (conn.kind === "removed") return <Removed waiting={waiting} onClear={clearWaiting} />;

  const connected = conn.kind === "connected";
  const sheetView = sheet && (
    <Sheet
      device={device}
      connection={conn}
      waiting={waiting}
      version={services.version}
      onClose={() => setSheet(false)}
      onDisconnect={async () => {
        const { reached } = await engine.disconnect();
        setSheet(false);
        onUnpaired(!reached);
      }}
    />
  );

  if (view.name === "keep") {
    return (
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
      />
    );
  }
  if (view.name === "checkin") {
    return (
      <CheckIn
        engine={engine}
        queue={services.queue}
        connection={conn}
        deviceName={device.deviceName}
        now={services.now}
        part={view.part}
        onBack={back}
        onQueued={refreshWaiting}
      />
    );
  }
  if (view.name === "journal") {
    return (
      <Journal
        engine={engine}
        queue={services.queue}
        connection={conn}
        computerName={
          conn.kind === "connected"
            ? conn.hello.computerName
            : (device.computerName ?? t("phone.home.computer"))
        }
        day={view.day}
        today={view.today}
        onBack={back}
        onDay={(day, today) => go({ name: "journal", day, today })}
        onOpen={openItem}
        onWhich={(q, day, today) => {
          question.current = q;
          go({ name: "which", id: q.id, day, today });
        }}
        onReflect={(day, today) =>
          go({
            name: "lookback",
            kind: "day",
            start: day,
            end: day,
            write: true,
            today,
          })
        }
        onReflection={(day, today) =>
          go({
            name: "lookback",
            kind: "day",
            start: day,
            end: day,
            write: false,
            today,
          })
        }
        onKeep={() => go({ name: "keep" })}
        onCheckIn={() => go({ name: "checkin" })}
      />
    );
  }
  if (view.name === "lookback") {
    return (
      <LookBack
        engine={engine}
        period={{ kind: view.kind, start: view.start, end: view.end }}
        write={view.write}
        today={view.today}
        now={services.now}
        onStarted={() => replace({ ...view, write: false })}
        onBack={back}
        onToday={() => go({ name: "journal", day: null, today: view.today })}
        onItem={openItem}
        onReceipt={open}
      />
    );
  }
  if (view.name === "which") {
    return (
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
  }
  if (view.name === "ask") {
    return (
      <Ask
        model={ask.model}
        connected={connected}
        onAsk={(q) => void ask.ask(q)}
        onStop={() => void ask.stop()}
        onOpen={open}
        onBack={back}
        onSeeChange={() => void ask.seeChange()}
        onAskHere={() => void ask.askHere()}
      />
    );
  }
  if (view.name === "page") {
    return (
      <PageScreen
        engine={engine}
        queue={services.queue}
        connected={connected}
        now={services.now}
        onQueued={refreshWaiting}
        onOpen={open}
        onJournal={(day) => go({ name: "journal", day, today: null })}
        onBack={back}
      />
    );
  }
  if (view.name === "original") {
    return <Original engine={engine} itemId={view.id} quote={quote.current} onBack={back} />;
  }

  const toKeep = () => go({ name: "keep" });
  const openMenu = () => setSheet(true);
  if (conn.kind === "turnedOff") {
    return (
      <>
        <TurnedOff device={device} waiting={waiting} now={now} onKeep={toKeep} onMenu={openMenu} />
        {sheetView}
      </>
    );
  }
  if (conn.kind === "outdated") {
    return <Outdated onScan={() => onRelink("pair")} onType={() => onRelink("type")} />;
  }
  return (
    <>
      <Home
        device={device}
        connection={conn}
        waiting={waiting}
        headline={headline}
        needsPage={noPage}
        sentJustNow={sent}
        now={now}
        onAsk={() => go({ name: "ask" })}
        onKeep={toKeep}
        onPage={() => go({ name: "page" })}
        onJournal={() => go({ name: "journal", day: null, today: null })}
        onMenu={openMenu}
        onRelink={onRelink}
      />
      {sheetView}
    </>
  );
}

export { t };
