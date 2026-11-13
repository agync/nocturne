// The services of the scripted build. `?s=<name>` picks what the phone starts as; nothing here
// is part of the shipped app (the build drops this module unless `VITE_ENGINE` is `phone-mock`).

import { generateDeviceKey, pairingLink, randomBytes } from "../crypto";
import type { PairedDevice } from "../engine";
import { createMockPhoneEngine, type MockScript } from "../mock";
import type { PairingResult } from "../pairing";
import { memoryQueue } from "../queue";
import type { PairingLike, Services } from "../services";
import { detectPlatform } from "../services";
import { memoryKeyStore } from "./memoryStore";

const SCENARIOS = [
  "connected",
  "connecting",
  "asleep",
  "asleepMostPrivate",
  "cantConnect",
  "turnedOff",
  "outdated",
  "removed",
  "welcome",
  "welcomeWaiting",
  "typed",
  "pair",
  "pairWaiting",
  "pairAllowed",
  "pairRefused",
  "pairExpired",
  "pairSpent",
  "iphone",
  "tooOld",
  "scanPair",
  "relinkWords",
  "relinkHere",
  "noPage",
] as const;
export type Scenario = (typeof SCENARIOS)[number];

const HOST = "amber-otter-lamp-quiet.trycloudflare.com";

class MockPairing implements PairingLike {
  spentAlready = false;
  constructor(
    private readonly outcome: "wait" | "allowed" | "refused" | "expired" | "spent",
    private readonly device: () => Promise<PairedDevice>,
  ) {}
  async connect(_name: string, onWords: (w: string[]) => void): Promise<PairingResult> {
    this.spentAlready = true;
    await new Promise((r) => setTimeout(r, 150));
    onWords(["harbour", "maple", "lantern"]);
    if (this.outcome === "wait") return new Promise(() => undefined);
    await new Promise((r) => setTimeout(r, 400));
    if (this.outcome === "allowed") return { kind: "allowed", device: await this.device() };
    return { kind: this.outcome };
  }
}

export async function mockServices(): Promise<Services> {
  // The end-to-end tests reach the real WebCrypto and storage code through these.
  const crypto = await import("../crypto");
  const keys = await import("../keys");
  (window as unknown as { __phone: object }).__phone = { crypto, keys };
  const params = new URLSearchParams(location.search);
  const scenario = (params.get("s") ?? "connected") as Scenario;
  const waitingCount = Number(
    params.get("waiting") ?? (scenario === "asleep" || scenario === "asleepMostPrivate" ? 2 : 0),
  );
  const key = await generateDeviceKey();
  const cpub = (await generateDeviceKey()).publicKey;
  const device: PairedDevice = {
    deviceId: randomBytes(16),
    privateKey: key.privateKey,
    publicKey: key.publicKey,
    cpub,
    homeOrigin: `https://${HOST}`,
    address: HOST,
    deviceName: "Pixel",
    computerName: "Home computer",
    connection: scenario === "asleepMostPrivate" ? "mostPrivate" : "simple",
    version: "0.1.0",
    pairedAt: 0,
    lastConnectedAt: Date.parse("2026-11-13T07:48:00"),
  };

  const paired = ![
    "welcome",
    "welcomeWaiting",
    "typed",
    "pair",
    "pairWaiting",
    "pairAllowed",
    "pairRefused",
    "pairExpired",
    "pairSpent",
    "iphone",
    "tooOld",
    "scanPair",
    "relinkHere",
  ].includes(scenario);
  const store = memoryKeyStore(paired ? device : null);
  const queue = memoryQueue();
  const waiting = scenario === "welcomeWaiting" ? Math.max(waitingCount, 2) : waitingCount;
  for (let i = 0; i < waiting; i += 1) {
    await queue.add({
      clientId: `seed-${i}`,
      kind: "text",
      text: `Waiting note ${i + 1}`,
      size: 10,
      createdAt: i,
    });
  }

  const connection: MockScript["connection"] =
    scenario === "removed"
      ? "removed"
      : scenario === "asleepMostPrivate"
        ? "asleep"
        : scenario === "turnedOff" ||
            scenario === "outdated" ||
            scenario === "asleep" ||
            scenario === "cantConnect" ||
            scenario === "connecting"
          ? scenario
          : "connected";
  const script: MockScript = {
    connection,
    noPage: scenario === "noPage",
    pace: Number(params.get("pace") ?? 60),
    hello: { connection: scenario === "asleepMostPrivate" ? "mostPrivate" : "simple" },
  };

  const outcome =
    scenario === "pairWaiting"
      ? "wait"
      : scenario === "pairRefused"
        ? "refused"
        : scenario === "pairExpired"
          ? "expired"
          : scenario === "pairSpent"
            ? "spent"
            : "allowed";

  const pairing = () =>
    new MockPairing(outcome, async () => {
      await store.save(device);
      return device;
    });

  const pairing_ = [
    "pair",
    "pairWaiting",
    "pairAllowed",
    "pairRefused",
    "pairExpired",
    "pairSpent",
    "iphone",
  ].includes(scenario);
  const s = randomBytes(15);
  const hash = pairing_ ? new URL(pairingLink(HOST, s, cpub)).hash : "";
  const platform = detectPlatform();

  return {
    store,
    queue,
    engineFor: () => ({
      ...createMockPhoneEngine(script),
      disconnect: async () => {
        await store.clear();
      },
    }),
    pairFromLink: async () => pairing(),
    pairFromCode: async () => pairing(),
    relink: async () => ({ ok: true, address: HOST }),
    scanner: { loadJsqr: async () => () => null },
    persist: async () => true,
    platform: scenario === "iphone" ? { ios: true, standalone: false } : platform,
    host: location.host,
    origin: location.origin,
    now: () => Date.parse("2026-11-13T14:05:00"),
    version: "0.1.0",
    scripted: true,
    unsupported: scenario === "tooOld" ? ["WebCrypto"] : undefined,
    location: {
      pathname: pairing_
        ? "/p"
        : scenario === "typed"
          ? "/p"
          : scenario === "relinkHere"
            ? "/r"
            : "/",
      hash: scenario === "relinkHere" ? `#${location.host}` : hash,
      search: location.search,
    },
  };
}
