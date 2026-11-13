import { useCallback, useEffect, useRef, useState } from "react";
import type { PairedDevice } from "./engine";
import { offerFromFragment, PairingError } from "./pairing";
import { addressFromRelinkText, addressFromWords } from "./relink";
import {
  AddToHomeScreen,
  RelinkHere,
  RelinkWords,
  Scan,
  TooOld,
  TypedCode,
  Welcome,
} from "./screens/Entry";
import { Pair } from "./screens/Pair";
import { Paired } from "./screens/Paired";
import type { PairingLike, Services } from "./services";
import { missingFeatures } from "./support";

type Screen =
  | { name: "loading" }
  | { name: "tooOld" }
  | { name: "welcome" }
  | { name: "iphone"; next: Screen }
  | { name: "typed"; error: "bad" | "noAnswer" | null; busy: boolean }
  | { name: "scanPair" }
  | { name: "pair"; pairing: PairingLike }
  | { name: "scanRelink" }
  | { name: "relinkWords"; busy: boolean; error: boolean }
  | { name: "relinkHere" }
  | { name: "paired"; device: PairedDevice };

/** Fragment and path of the pairing link, or null. Only the page's own host is accepted. */
function offerFromScan(text: string, host: string) {
  try {
    const url = new URL(text.trim());
    if (url.protocol !== "https:" || url.host !== host || url.pathname !== "/p") return null;
    return offerFromFragment(url.hash);
  } catch {
    return null;
  }
}

export function App({ services }: { services: Services }) {
  const [screen, setScreen] = useState<Screen>({ name: "loading" });
  const [waiting, setWaiting] = useState(0);
  const [deviceVersion, setDeviceVersion] = useState(0);
  const origin = useRef<Screen | null>(null);

  const home = useCallback(async (): Promise<Screen> => {
    const device = await services.store.device();
    if (device) return { name: "paired", device };
    setWaiting(await services.queue.count());
    return { name: "welcome" };
  }, [services]);

  useEffect(() => {
    let gone = false;
    (async () => {
      const missing = services.unsupported ?? missingFeatures();
      if (missing.length > 0) return setScreen({ name: "tooOld" });
      const { pathname, hash } = services.location;
      const device = await services.store.device();
      let next: Screen;
      if (pathname === "/p" && !device) {
        const offer = offerFromFragment(hash);
        // The fragment is the secret; it leaves the address bar as soon as it is read.
        history.replaceState(null, "", "/p");
        if (offer) {
          const pairing = await services.pairFromLink(offer);
          next = { name: "pair", pairing };
          if (services.platform.ios && !services.platform.standalone)
            next = { name: "iphone", next };
        } else {
          next = { name: "typed", error: null, busy: false };
          if (services.platform.ios && !services.platform.standalone)
            next = { name: "iphone", next };
        }
      } else if (pathname === "/p" && device) {
        const offer = offerFromFragment(hash);
        history.replaceState(null, "", "/");
        next = offer
          ? { name: "pair", pairing: await services.pairFromLink(offer) }
          : { name: "paired", device };
      } else if (pathname === "/r") {
        const address = addressFromRelinkText(`https://${services.host}/r${hash}`);
        history.replaceState(null, "", "/");
        if (device && address) {
          await services.relink(address);
          next = { name: "paired", device: (await services.store.device()) ?? device };
        } else {
          next = device ? { name: "paired", device } : { name: "relinkHere" };
        }
      } else if (device) {
        next = { name: "paired", device };
      } else {
        setWaiting(await services.queue.count());
        next = { name: "welcome" };
      }
      if (!gone) setScreen(next);
    })();
    return () => {
      gone = true;
    };
  }, [services]);

  const toWelcome = async () => setScreen(await home());

  switch (screen.name) {
    case "loading":
      return null;
    case "tooOld":
      return <TooOld />;
    case "welcome":
      return (
        <Welcome
          waiting={waiting}
          onScan={() => setScreen({ name: "scanPair" })}
          onType={() => setScreen({ name: "typed", error: null, busy: false })}
          onClear={async () => {
            await services.queue.clear();
            setWaiting(0);
          }}
        />
      );
    case "iphone":
      return (
        <AddToHomeScreen onDone={() => void toWelcome()} onSkip={() => setScreen(screen.next)} />
      );
    case "typed":
      return (
        <TypedCode
          busy={screen.busy}
          error={screen.error}
          onBack={() => void toWelcome()}
          onSubmit={async (code) => {
            setScreen({ name: "typed", error: null, busy: true });
            try {
              const pairing = await services.pairFromCode(code);
              setScreen({ name: "pair", pairing });
            } catch (e) {
              if (!(e instanceof PairingError)) throw e;
              setScreen({
                name: "typed",
                error: e.kind === "badCode" ? "bad" : "noAnswer",
                busy: false,
              });
            }
          }}
        />
      );
    case "scanPair":
      return (
        <Scan
          purpose="pair"
          env={services.scanner}
          accept={(text) => {
            const offer = offerFromScan(text, services.host);
            if (!offer) return false;
            void services
              .pairFromLink(offer)
              .then((pairing) => setScreen({ name: "pair", pairing }));
            return true;
          }}
          onSwitch={() => setScreen({ name: "typed", error: null, busy: false })}
          onCancel={() => void toWelcome()}
        />
      );
    case "pair":
      return (
        <Pair
          services={services}
          pairing={screen.pairing}
          onExit={() => void toWelcome()}
          onPaired={async (device) => {
            history.replaceState(null, "", "/");
            setScreen(device ? { name: "paired", device } : await home());
          }}
        />
      );
    case "scanRelink":
      return (
        <Scan
          purpose="relink"
          env={services.scanner}
          accept={(text) => {
            const address = addressFromRelinkText(text);
            if (!address) return false;
            void services.relink(address).then(async (result) => {
              if (result.ok) {
                setDeviceVersion((n) => n + 1);
                setScreen(await home());
              } else {
                setScreen({ name: "relinkWords", busy: false, error: true });
              }
            });
            return true;
          }}
          onSwitch={() => setScreen({ name: "relinkWords", busy: false, error: false })}
          onCancel={async () => setScreen(await home())}
        />
      );
    case "relinkWords":
      return (
        <RelinkWords
          busy={screen.busy}
          error={screen.error}
          onBack={async () => setScreen(await home())}
          onScan={() => setScreen({ name: "scanRelink" })}
          onSubmit={async (words) => {
            const address = addressFromWords(words);
            if (!address) return setScreen({ name: "relinkWords", busy: false, error: true });
            setScreen({ name: "relinkWords", busy: true, error: false });
            const result = await services.relink(address);
            if (result.ok) {
              setDeviceVersion((n) => n + 1);
              setScreen(await home());
            } else {
              setScreen({ name: "relinkWords", busy: false, error: true });
            }
          }}
        />
      );
    case "relinkHere":
      return <RelinkHere />;
    case "paired":
      return (
        <Paired
          services={services}
          initial={screen.device}
          deviceVersion={deviceVersion}
          onUnpaired={() => void toWelcome()}
          onRelink={(kind) => {
            origin.current = screen;
            setScreen(
              kind === "scan"
                ? { name: "scanRelink" }
                : { name: "relinkWords", busy: false, error: false },
            );
          }}
        />
      );
  }
}
