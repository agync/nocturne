import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { App } from "../App";
import { Pair } from "../screens/Pair";
import type { PairingLike } from "../services";
import { AddressStep, CameraOff, CodeStep, NativeWelcome } from "./Entry";
import { openAppSettings, scanCode } from "./scanner";
import { checkAddress, type NativeServices, pairScanned, pairTyped } from "./services";

/** The project's site, opened in the phone's browser from Welcome. */
const SITE = "https://nocturne.is/";

type Screen =
  | { name: "loading" }
  | { name: "welcome"; unreached: boolean; wrongCode?: boolean }
  | { name: "cameraOff" }
  | { name: "address"; text: string; busy: boolean; error: "incomplete" | "noAnswer" | null }
  | { name: "code"; address: string; busy: boolean; error: "bad" | "noAnswer" | null }
  | { name: "pair"; pairing: PairingLike }
  | { name: "paired" };

/**
 * The phone app until it is paired: Welcome, scanning in the app, and typing the address then
 * the code. Once paired, the phone screens take over, and a pairing that goes away (removed,
 * disconnected) comes back here.
 */
export function NativeRoot({ services }: { services: NativeServices }) {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [waiting, setWaiting] = useState(0);

  const home = useCallback(
    async (unreached = false) => {
      const device = await services.store.device().catch(() => null);
      if (device) return setScreen({ name: "paired" });
      setWaiting(await services.queue.count().catch(() => 0));
      setScreen({ name: "welcome", unreached });
    },
    [services],
  );

  useEffect(() => {
    void home();
    return services.store.subscribe(() => void home());
  }, [services, home]);

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
    case "welcome":
      return (
        <NativeWelcome
          waiting={waiting}
          unreached={screen.unreached}
          wrongCode={screen.wrongCode ?? false}
          onScan={() => void scan()}
          onType={toAddress}
          onSite={() => void invoke("plugin:opener|open_url", { url: SITE }).catch(() => undefined)}
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
          onExit={() => void home()}
          onPaired={() => void home()}
        />
      );
    case "paired":
      return <App services={services} />;
  }
}
