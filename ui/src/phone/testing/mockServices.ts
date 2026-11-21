// The services of the scripted build. `?s=<name>` picks what the phone starts as; nothing here
// is part of the shipped app (the build drops this module unless `VITE_ENGINE` is `phone-mock`).

import { generateDeviceKey, pairingLink, randomBytes } from "../crypto";
import type { PairedDevice } from "../engine";
import { createMockPhoneEngine, type MockScript } from "../mock";
import type { PairingResult } from "../pairing";
import { memoryQueue } from "../queue";
import type { PairingLike, Services } from "../services";
import { detectPlatform } from "../services";
import { answeredDay, emptyDay, stamp, TODAY, WEEK_PERIOD, YESTERDAY } from "./journalFixtures";
import { memoryKeyStore } from "./memoryStore";

const SCENARIOS = [
  "connected",
  "connecting",
  "asleep",
  "asleepMostPrivate",
  "cantConnect",
  "turnedOff",
  "outdated",
  "computerOutdated",
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
  "journalAsleep",
  "journalEmpty",
  "checkIn",
  "lookBackDay",
  "lookBackWeek",
  "lookBackToday",
  "whichGone",
  "pageQuestion",
  "pageElsewhere",
  "pageWaiting",
  "pageRefused",
  "journalAnswer",
] as const;
export type Scenario = (typeof SCENARIOS)[number];

/** The screen a scenario opens on, instead of Home. The app reads it from the history entry. */
function startView(scenario: Scenario): object | null {
  switch (scenario) {
    case "journalAsleep":
    case "journalEmpty":
      return { name: "journal", day: null, today: TODAY };
    case "checkIn":
      return { name: "checkin" };
    case "lookBackDay":
      return {
        name: "lookback",
        kind: "day",
        start: YESTERDAY,
        end: YESTERDAY,
        write: false,
        today: TODAY,
      };
    case "lookBackWeek":
      return {
        name: "lookback",
        kind: WEEK_PERIOD.kind,
        start: WEEK_PERIOD.start,
        end: WEEK_PERIOD.end,
        write: false,
        today: TODAY,
      };
    case "whichGone":
      return { name: "which", id: "snq-gone", day: TODAY, today: TODAY };
    case "pageQuestion":
    case "pageElsewhere":
    case "pageWaiting":
    case "pageRefused":
      return { name: "page" };
    case "journalAnswer":
      return { name: "journal", day: null, today: TODAY };
    case "lookBackToday":
      return { name: "lookback", kind: "day", start: TODAY, end: TODAY, write: true, today: TODAY };
    default:
      return null;
  }
}

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
  const start = startView(scenario);
  if (start) history.replaceState({ view: start }, "");
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

  if (scenario === "journalAsleep") {
    await queue.add({
      clientId: "seed-voice",
      kind: "recording",
      blob: new Blob([new Uint8Array(8)], { type: "audio/wav" }),
      size: 44 + 48 * 32_000,
      createdAt: Date.parse(stamp(TODAY, "20:40").replace("Z", "")),
    });
    await queue.add({
      clientId: "seed-checkin",
      kind: "checkin",
      checkIn: {
        feeling: { part: "settled", point: { pleasant: 0.5, energy: -0.5 }, words: ["calm"] },
        line: null,
        urge: null,
        replaces: null,
      },
      size: 1,
      createdAt: Date.parse(stamp(TODAY, "20:44").replace("Z", "")),
    });
  }

  const connection: MockScript["connection"] =
    scenario === "removed"
      ? "removed"
      : scenario === "asleepMostPrivate" || scenario === "journalAsleep"
        ? "asleep"
        : scenario === "turnedOff" ||
            scenario === "outdated" ||
            scenario === "computerOutdated" ||
            scenario === "asleep" ||
            scenario === "cantConnect" ||
            scenario === "connecting"
          ? scenario
          : "connected";
  const script: MockScript = {
    connection,
    noPage: scenario === "noPage",
    keepFails: scenario === "pageWaiting",
    answeredElsewhere: scenario === "pageElsewhere",
    answerRefused: scenario === "pageRefused",
    pace: Number(params.get("pace") ?? 60),
    onlineChanged: params.get("onlineChanged") === "1",
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
      ...(scenario === "journalEmpty" ? { getJournalDay: async () => emptyDay(TODAY, true) } : {}),
      ...(scenario === "journalAnswer" ? { getJournalDay: async () => answeredDay() } : {}),
      disconnect: async () => {
        await store.clear();
        return { reached: scenario !== "asleep" && scenario !== "cantConnect" };
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
