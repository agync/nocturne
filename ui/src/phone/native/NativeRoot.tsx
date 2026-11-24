import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { t } from "../../copy/t";
import { HomeApp } from "../alone/HomeApp";
import { createBackupApi, onPhone } from "../alone/nightly";
import { Restore } from "../alone/screens/Backup";
import { MoveFlow, nativeMoveApi } from "../alone/screens/MoveFlow";
import { NightlyRestore } from "../alone/screens/NightlyRestore";
import { Permissions } from "../alone/screens/Permissions";
import { AloneWelcome, Damaged, Start } from "../alone/screens/Welcome";
import { cleanDeviceName } from "../crypto";
import type { PairedDevice } from "../engine";
import { Pair } from "../screens/Pair";
import type { PairingLike } from "../services";
import { CLOCK_CHANGED } from "./clock";
import { AddressStep, CameraOff, CodeStep, NativeWelcome } from "./Entry";
import { watchLaunch } from "./launch";
import { NativeApp } from "./NativeApp";
import { openAppSettings, scanCode } from "./scanner";
import { CalendarConnect } from "./screens/CalendarConnect";
import { HealthConnect } from "./screens/HealthConnect";
import { Rationale } from "./screens/Rationale";
import { queueShared } from "./screens/ShareSheet";
import { checkAddress, type NativeServices, pairScanned, pairTyped } from "./services";
import { nativeChannel, type StillGranted, watchFront } from "./sources";

/** The project's site, opened in the phone's browser from Welcome. */
const SITE = "https://nocturne.is/";

type Screen =
  | { name: "loading" }
  | { name: "first" }
  | { name: "permissions" }
  | { name: "start"; busy: boolean; failed: boolean }
  | { name: "restore" }
  | { name: "restoreFile" }
  | { name: "damaged" }
  | { name: "alone"; offer: boolean }
  | { name: "welcome"; unreached: boolean; wrongCode?: boolean }
  | { name: "cameraOff" }
  | { name: "address"; text: string; busy: boolean; error: "incomplete" | "noAnswer" | null }
  | { name: "code"; address: string; busy: boolean; error: "bad" | "noAnswer" | null }
  | { name: "pair"; pairing: PairingLike }
  /** `resumed`: opened again part way, rather than just paired. */
  | { name: "move"; resumed?: boolean }
  | { name: "paired"; device: PairedDevice; offer: boolean };

/** A page opened by its own address: from Health Connect, or from This phone. */
type Route = "rationale" | "calendar" | "health";

export function routeOf(hash: string): Route | null {
  const r = hash.replace(/^#\/?/, "");
  return r === "rationale" || r === "calendar" || r === "health" ? r : null;
}

const openSite = () => void invoke("plugin:opener|open_url", { url: SITE }).catch(() => undefined);

/**
 * The phone app until it has a home for its memory: Welcome, starting on this phone or bringing
 * back a backup, and pairing (scanning in the app, or typing the address then the code). A phone
 * alone opens on its own memory; a paired phone on its computer's. A pairing that goes away
 * (removed, disconnected) comes back here.
 */
export function NativeRoot({ services }: { services: NativeServices }) {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [waiting, setWaiting] = useState(0);
  // A changed 12 or 24-hour setting draws the times again.
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    window.addEventListener(CLOCK_CHANGED, redraw);
    return () => window.removeEventListener(CLOCK_CHANGED, redraw);
  }, []);
  const [route, setRoute] = useState<Route | null>(() => routeOf(location.hash));
  const [name, setName] = useState(() => cleanDeviceName(t("phone.pair.default")));
  // Just after Delete everything, Welcome says so until the person goes on.
  const [deleted, setDeleted] = useState(false);
  const [deletedLine, setDeletedLine] = useState<string | undefined>(undefined);
  // And what the phone still lets Nocturne use, which is all that line may name.
  const [granted, setGranted] = useState<StillGranted | null>(null);
  // A restore says where the memory lives before its own "back on this phone" is read.
  const shown = useRef(screen.name);
  shown.current = screen.name;
  // The phone holds its own memory: pairing is the way to move it.
  const [alone, setAlone] = useState(false);

  // A deletion that waits for an account the phone no longer reaches says which one it waits for.
  const welcomeDeleted = screen.name === "first" && deleted && deletedLine !== undefined;
  useEffect(() => {
    if (!welcomeDeleted) return;
    let live = true;
    void (services.backup ?? createBackupApi())
      .status()
      .then((s) => {
        if (live && s.deletePending && s.deleteElsewhere)
          setDeletedLine(
            onPhone(
              services.platform.ios,
              "phone.backup.delete.elsewhere.ios",
              "phone.backup.delete.elsewhere.android",
            ),
          );
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [welcomeDeleted, services]);

  useEffect(() => {
    void services.deviceName?.().then((own) => {
      const clean = own ? cleanDeviceName(own.trim()) : "";
      if (clean !== "") setName(clean);
    });
  }, [services]);

  const home = useCallback(
    async (unreached = false, offer = false) => {
      // A memory moving to a computer says itself where it goes next.
      if (shown.current === "move") return;
      const mode = await services.home.mode();
      setAlone(mode === "alone");
      // A damaged memory is found when it is first opened: open it before asking.
      if (mode === "alone") await services.home.stats().catch(() => undefined);
      if (await services.home.damaged()) return setScreen({ name: "damaged" });
      // A phone alone that holds a pairing is part way through moving its memory (pairing
      // undone on every other way out): the move carries on where it was.
      if (mode === "alone" && (await (services.move ?? nativeMoveApi).pending()))
        return setScreen({ name: "move", resumed: true });
      if (mode === "alone")
        return setScreen((now) =>
          now.name === "alone" && !offer ? now : { name: "alone", offer },
        );
      const device = await services.store.device().catch(() => null);
      if (device)
        return setScreen((now) =>
          now.name === "paired" && now.device.address === device.address && !offer
            ? now
            : { name: "paired", device, offer },
        );
      setWaiting(await services.queue.count().catch(() => 0));
      setScreen(unreached ? { name: "welcome", unreached } : { name: "first" });
    },
    [services],
  );

  useEffect(() => {
    void home();
    const stopStore = services.store.subscribe(() => void home());
    const stopMode = services.home.onMode(() => {
      if (shown.current !== "restore" && shown.current !== "restoreFile") void home();
    });
    return () => {
      stopStore();
      stopMode();
    };
  }, [services, home]);

  // From the pairing Welcome, the phone's back gesture returns to the first Welcome.
  useEffect(() => {
    const pairing = new Set(["welcome", "cameraOff", "address", "code"]);
    const onPop = () => {
      if (pairing.has(shown.current)) void home();
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [home]);

  useEffect(() => {
    const changed = () => setRoute(routeOf(location.hash));
    window.addEventListener("hashchange", changed);
    const stop = watchFront((r) => {
      location.hash = `#/${r}`;
    });
    return () => {
      window.removeEventListener("hashchange", changed);
      stop();
    };
  }, []);

  // Not paired: what another app shares waits on this phone, and Welcome counts it.
  const unpaired = screen.name !== "paired" && screen.name !== "loading" && screen.name !== "alone";
  useEffect(() => {
    if (!unpaired) return;
    return watchLaunch(
      () => undefined,
      (shared) =>
        void queueShared(shared, services.queue, services.now).then(async () =>
          setWaiting(await services.queue.count().catch(() => 0)),
        ),
    );
  }, [unpaired, services]);

  const leave = () => {
    history.replaceState(null, "", location.pathname + location.search);
    setRoute(null);
  };

  // The rationale reads without pairing; the calendar and Health need a computer to go to.
  if (route === "rationale") return <Rationale onBack={leave} alone={screen.name === "alone"} />;
  const homed = screen.name === "paired" || screen.name === "alone";
  if (homed && route === "calendar")
    return (
      <CalendarConnect channel={nativeChannel} onDone={leave} alone={screen.name === "alone"} />
    );
  if (homed && route === "health")
    return <HealthConnect channel={nativeChannel} onDone={leave} alone={screen.name === "alone"} />;

  const scan = async () => {
    const got = await scanCode();
    if (got.kind === "cameraOff") return setScreen({ name: "cameraOff" });
    if (got.kind === "cancelled") return void home();
    try {
      setScreen({ name: "pair", pairing: await pairScanned(got.text) });
    } catch {
      setScreen({ name: "welcome", unreached: false, wrongCode: true });
    }
  };

  const toAddress = () => setScreen({ name: "address", text: "", busy: false, error: null });

  switch (screen.name) {
    case "loading":
      return null;
    case "first":
      return (
        <AloneWelcome
          deleted={deleted}
          deletedLine={deletedLine}
          granted={granted}
          onPermissions={() => setScreen({ name: "permissions" })}
          onStart={() => {
            setDeleted(false);
            setScreen({ name: "start", busy: false, failed: false });
          }}
          onConnect={() => {
            setDeleted(false);
            history.pushState({ root: "pairing" }, "");
            setScreen({ name: "welcome", unreached: false });
          }}
          onBackup={() => {
            setDeleted(false);
            setScreen({ name: "restore" });
          }}
        />
      );
    case "permissions":
      return (
        <Permissions
          ios={services.platform.ios}
          fromWelcome
          onBack={() => setScreen({ name: "first" })}
        />
      );
    case "start":
      return (
        <Start
          busy={screen.busy}
          failed={screen.failed}
          onStart={async () => {
            setScreen({ name: "start", busy: true, failed: false });
            try {
              await services.home.startAlone(name);
              setScreen({ name: "alone", offer: true });
            } catch {
              setScreen({ name: "start", busy: false, failed: true });
            }
          }}
        />
      );
    case "restore":
      return (
        <NightlyRestore
          api={services.backup ?? createBackupApi()}
          ios={services.platform.ios}
          holds={false}
          freeSpace={services.home.freeSpace}
          onFile={() => setScreen({ name: "restoreFile" })}
          now={services.now}
          onBack={() => void home()}
          onDone={() => void home()}
        />
      );
    case "restoreFile":
      return (
        <Restore
          pickFile={services.home.chooseBackupFile}
          check={services.home.checkBackup}
          restore={services.home.restoreBackup}
          stop={services.home.stopBackup}
          letGo={services.home.letGoBackupFile}
          onBack={() => void home()}
          onDone={() => void home()}
        />
      );
    case "damaged":
      return <Damaged onBackup={() => setScreen({ name: "restore" })} onHelp={openSite} />;
    case "alone":
      return (
        <HomeApp
          key={screen.offer ? "offer" : "home"}
          services={services}
          channel={nativeChannel}
          offer={screen.offer}
          name={name}
          onLost={() => void home()}
          onDeleted={(line) => {
            void services.granted().then((now) => {
              setGranted(now);
              setDeletedLine(line);
              setDeleted(true);
              void home();
            });
          }}
          onScan={() => void scan()}
        />
      );
    case "welcome":
      return (
        <NativeWelcome
          waiting={waiting}
          unreached={screen.unreached}
          wrongCode={screen.wrongCode ?? false}
          onScan={() => void scan()}
          onType={toAddress}
          onSite={openSite}
          onClear={async () => {
            await services.queue.clear();
            setWaiting(0);
          }}
        />
      );
    case "cameraOff":
      return (
        <CameraOff
          onSettings={() => void openAppSettings()}
          onType={toAddress}
          onBack={() => void home()}
        />
      );
    case "address":
      return (
        <AddressStep
          initial={screen.text}
          busy={screen.busy}
          error={screen.error}
          onScan={() => void scan()}
          onBack={() => void home()}
          onNext={async (text) => {
            setScreen({ name: "address", text, busy: true, error: null });
            try {
              const address = await checkAddress(text);
              setScreen({ name: "code", address, busy: false, error: null });
            } catch (e) {
              const error = e === "badAddress" ? "incomplete" : "noAnswer";
              setScreen({ name: "address", text, busy: false, error });
            }
          }}
        />
      );
    case "code":
      return (
        <CodeStep
          busy={screen.busy}
          error={screen.error}
          onScan={() => void scan()}
          onBack={() =>
            setScreen({ name: "address", text: screen.address, busy: false, error: null })
          }
          onConnect={async (code) => {
            const address = screen.address;
            setScreen({ name: "code", address, busy: true, error: null });
            try {
              setScreen({ name: "pair", pairing: await pairTyped(address, code) });
            } catch (e) {
              const kind = (e as { kind?: string }).kind;
              setScreen({
                name: "code",
                address,
                busy: false,
                error: kind === "badCode" ? "bad" : "noAnswer",
              });
            }
          }}
        />
      );
    case "pair":
      return (
        <Pair
          services={services}
          pairing={screen.pairing}
          alone={alone}
          onExit={() => void home()}
          onPaired={async () => {
            // A phone that holds its own memory offers to move it before it works with the
            // computer: one home at a time.
            if ((await services.home.mode()) === "alone") return setScreen({ name: "move" });
            void home(false, true);
          }}
        />
      );
    case "move": {
      const leave = () => {
        // What the phone alone recorded in history means nothing to the app that comes next.
        history.replaceState(null, "", location.pathname + location.search);
        shown.current = "loading";
        setScreen({ name: "loading" });
        void home();
      };
      return (
        <MoveFlow
          api={services.move ?? nativeMoveApi}
          resumed={screen.resumed}
          onExit={leave}
          onDone={leave}
        />
      );
    }
    case "paired":
      return (
        <NativeApp
          key={`${screen.device.address}${screen.offer ? "-offer" : ""}`}
          services={services}
          channel={nativeChannel}
          initial={screen.device}
          offer={screen.offer}
          onUnpaired={(unreached) => {
            services.store.changed();
            void home(unreached);
          }}
        />
      );
  }
}
