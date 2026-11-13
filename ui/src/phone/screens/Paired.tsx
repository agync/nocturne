import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { t } from "../../copy/t";
import type { Receipt } from "../../engine/bindings";
import type { PairedDevice } from "../engine";
import type { Services } from "../services";
import { refreshShellOnce } from "../shell";
import { Ask, useAskSession } from "./Ask";
import { Home, NewAddress, Removed, Sheet, TurnedOff } from "./Home";
import { Keep } from "./Keep";
import { Original } from "./Original";
import { PageScreen } from "./PageScreen";

type View =
  | { name: "home" }
  | { name: "keep" }
  | { name: "ask" }
  | { name: "page" }
  | { name: "original"; id: string; quote: string | null };

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
  const back = useCallback(() => {
    if (history.length > 1 && history.state?.view?.name !== "home") history.back();
    else {
      history.replaceState({ view: HOME }, "");
      setView(HOME);
    }
  }, []);
  return { view, go, back };
}

const RETRY_MS = 30_000;

interface PairedProps {
  services: Services;
  initial: PairedDevice;
  onUnpaired(): void;
  onRelink(kind: "scan" | "words"): void;
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
  const { view, go, back } = useView();
  const [sheet, setSheet] = useState(false);
  const [waiting, setWaiting] = useState(0);
  const [sent, setSent] = useState(0);
  const [headline, setHeadline] = useState<string | null>(null);
  const [noPage, setNoPage] = useState(false);
  const ask = useAskSession(engine);
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
      const quote = receipt.span?.type === "quote" ? receipt.span.text : null;
      go({ name: "original", id: receipt.itemId, quote });
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
        await engine.disconnect();
        setSheet(false);
        onUnpaired();
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
      />
    );
  }
  if (view.name === "page") return <PageScreen engine={engine} onOpen={open} onBack={back} />;
  if (view.name === "original") {
    return <Original engine={engine} itemId={view.id} quote={view.quote} onBack={back} />;
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
  if (conn.kind === "newAddress") {
    return (
      <>
        <NewAddress
          waiting={waiting}
          onScan={() => onRelink("scan")}
          onWords={() => onRelink("words")}
          onKeep={toKeep}
          onMenu={openMenu}
        />
        {sheetView}
      </>
    );
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
        onMenu={openMenu}
      />
      {sheetView}
    </>
  );
}

export { t };
