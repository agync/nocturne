import type {
  DeviceView,
  LedgerEntry,
  PairingOffer,
  PairingPhase,
  PairingRequest,
  PrivacyView,
  QrCode,
  RemoteAction,
  RemoteAdvanced,
  RemoteConnection,
  RemoteEvent,
  RemoteProblem,
  RemoteResult,
  RemoteSettingsPatch,
  RemoteState,
  RemoteView,
} from "../bindings";
import { EngineError, type EngineEventName, type EngineEvents } from "../client";

const MINUTE = 60_000;
const HOUR = 3_600_000;
/** An offer lives this long, and a phone's request waits this long for a decision. */
const OFFER_MS = 5 * MINUTE;
const DECIDE_MS = 2 * MINUTE;

const DOWNLOAD_BYTES: Record<RemoteConnection, number> = { simple: 40e6, mostPrivate: 50e6 };
const HOST = "amber-otter-lamp-quiet.trycloudflare.com";
const RELINK_HOST = "violet-heron-sage-ember.trycloudflare.com";
const TAILNET_HOST = "nocturne-0000.tailnet.invalid";
const WORDS = ["harbour", "maple", "lantern"];

type Emit = <E extends EngineEventName>(name: E, payload: EngineEvents[E]) => void;

export interface RemoteOptions {
  /** True while the scripted parts (downloads, a phone arriving) move by themselves. */
  auto?: boolean;
  /** How long each scripted step takes, in milliseconds. */
  stepMs?: number;
  /** Which way the scripted phone pairs. */
  phone?: "scanned" | "typed";
  /** The state to start in; see `REMOTE_SCENARIOS`. */
  scenario?: string;
}

const clone = <T>(value: T): T => structuredClone(value);

/** A square that looks like a QR code, for drawing; it does not scan. */
export function fakeQr(seed: number, size = 29): QrCode {
  let x = seed || 1;
  const next = () => {
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    return (x >>> 0) % 100;
  };
  const rows: string[] = [];
  const finder = (r: number, c: number) => {
    const inside = (r0: number, c0: number) => r >= r0 && r < r0 + 7 && c >= c0 && c < c0 + 7;
    for (const [r0, c0] of [
      [0, 0],
      [0, size - 7],
      [size - 7, 0],
    ] as const) {
      if (inside(r0, c0)) {
        const dr = r - r0;
        const dc = c - c0;
        const ring = Math.max(Math.abs(dr - 3), Math.abs(dc - 3));
        return ring === 3 || ring <= 1 ? "1" : "0";
      }
      if (r >= r0 - 1 && r <= r0 + 7 && c >= c0 - 1 && c <= c0 + 7) return "0";
    }
    return null;
  };
  for (let r = 0; r < size; r += 1) {
    let row = "";
    for (let c = 0; c < size; c += 1) row += finder(r, c) ?? (next() < 48 ? "1" : "0");
    rows.push(row);
  }
  return { size, modules: rows.join("") };
}

interface Seeded {
  view: RemoteView;
  events: RemoteEvent[];
}

/** The states a visit can start from: `?mockAway=<name>` before the `#`. */
export const REMOTE_SCENARIOS = [
  "off",
  "offBefore",
  "downloading",
  "signIn",
  "approve",
  "allowLinks",
  "starting",
  "on",
  "onAlone",
  "inUse",
  "relink",
  "problem",
  "noInternet",
] as const;

const iso = (ms: number) => new Date(ms).toISOString();

/**
 * A computer for Away from home that does what the real one is specified to do, on a script:
 * downloads tick, the connection starts, a phone scans or types a code, the person decides.
 * Nothing here talks to anything.
 */
export class RemoteSim {
  private v: RemoteView;
  private log: RemoteEvent[] = [];
  private offer: PairingOffer | null = null;
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private counter = 0;
  private phoneNumber = 0;
  private readonly auto: boolean;
  private readonly stepMs: number;
  private readonly phone: "scanned" | "typed";
  /** How many times the sign-in page was asked for, for tests. */
  signInOpened = 0;
  /** How many times a new computer key was made, for tests. */
  keysMade = 0;
  helperBytes = 0;

  constructor(
    private readonly emit: Emit,
    private readonly now: () => number = Date.now,
    options: RemoteOptions = {},
  ) {
    this.auto = options.auto ?? true;
    this.stepMs = options.stepMs ?? 1500;
    this.phone = options.phone ?? "scanned";
    const seeded = this.seed(options.scenario ?? "off");
    this.v = seeded.view;
    this.log = seeded.events;
    this.resume();
  }

  /** A visit that starts mid-way carries on by itself, as the real connection would. */
  private resume(): void {
    const s = this.v.state;
    if (s.type === "downloading" && !s.paused) this.after(this.stepMs, () => this.advance());
    if (s.type === "starting") this.after(this.stepMs, () => this.advance());
    if (s.type === "needsSignIn") this.after(this.stepMs * 2, () => this.advance());
  }

  // Seeds

  private device(
    id: string,
    name: string,
    paired: number,
    used: number | null,
    over: Partial<DeviceView> = {},
  ): DeviceView {
    return {
      id,
      name,
      pairedAt: iso(paired),
      lastUsedAt: used === null ? null : iso(used),
      active: false,
      needsAddress: false,
      ...over,
    };
  }

  private seed(name: string): Seeded {
    const n = this.now();
    const base: RemoteView = {
      state: { type: "off" },
      connection: "simple",
      autoOff: "unused30Days",
      keepAwake: false,
      devices: [],
      sentToPhonesBytes: 0,
      lastOnAt: null,
      onSince: null,
      setUpBefore: false,
      installed: [],
      trial: true,
      pairing: null,
      computerName: "Kitchen laptop",
    };
    const dayStart = new Date(n);
    dayStart.setHours(0, 0, 0, 0);
    const today = (h: number, m: number) => dayStart.getTime() + (h * 60 + m) * MINUTE;
    const yesterday = (h: number, m: number) => today(h, m) - 24 * HOUR;
    const phones = (active: boolean): DeviceView[] => [
      this.device("pixel", "Pixel", today(9, 20) - 12 * 24 * HOUR, n - 20_000, { active }),
      this.device("work", "Work phone", n - 11 * 24 * HOUR, yesterday(18, 40)),
    ];
    const on: RemoteView = {
      ...base,
      state: { type: "on", address: HOST, machineName: null },
      installed: ["simple"],
      devices: phones(false),
      sentToPhonesBytes: 3.2e6,
      lastOnAt: iso(today(7, 2)),
      onSince: iso(today(7, 2)),
      setUpBefore: true,
    };
    const events: RemoteEvent[] = [
      this.event("Pixel", "keptPhoto", today(14, 5)),
      this.event("Pixel", "asked", today(14, 2)),
      this.event("Pixel", "readPage", today(12, 30)),
      this.event("Pixel", "allowed", today(9, 20)),
      this.event("Work phone", "keptVoiceNote", yesterday(18, 40)),
      this.event("Work phone", "asked", yesterday(18, 32), "ok", 2),
      this.event("Work phone", "tookNewAddress", yesterday(7, 15)),
    ];
    const withWrongCodes = [this.event(null, "wrongCode", today(3, 41), "refused", 5), ...events];
    switch (name) {
      case "offBefore":
        return {
          view: {
            ...base,
            devices: phones(false),
            setUpBefore: true,
            installed: ["simple"],
            lastOnAt: iso(yesterday(21, 10)),
            sentToPhonesBytes: 3.2e6,
          },
          events,
        };
      case "downloading":
        return {
          view: {
            ...base,
            state: { type: "downloading", receivedBytes: 18e6, totalBytes: 40e6, paused: false },
          },
          events: [],
        };
      case "signIn":
        return {
          view: {
            ...base,
            connection: "mostPrivate",
            installed: ["mostPrivate"],
            state: { type: "needsSignIn", url: "https://login.example/a/3f9c", step: "signIn" },
          },
          events: [],
        };
      case "approve":
        return {
          view: {
            ...base,
            connection: "mostPrivate",
            installed: ["mostPrivate"],
            state: { type: "needsSignIn", url: "https://login.example/a/3f9c", step: "approve" },
          },
          events: [],
        };
      case "allowLinks":
        return {
          view: {
            ...base,
            connection: "mostPrivate",
            installed: ["mostPrivate"],
            state: { type: "needsSignIn", url: "https://login.example/a/3f9c", step: "allowLinks" },
          },
          events: [],
        };
      case "starting":
        return {
          view: { ...base, installed: ["simple"], state: { type: "starting" } },
          events: [],
        };
      case "onAlone":
        return {
          view: { ...on, devices: [], sentToPhonesBytes: 0, setUpBefore: false },
          events: [],
        };
      case "on":
        return { view: on, events: withWrongCodes };
      case "inUse":
        return { view: { ...on, devices: phones(true) }, events: withWrongCodes };
      case "relink":
        return {
          view: {
            ...on,
            state: {
              type: "needsRelink",
              address: RELINK_HOST,
              pairingLink: `https://${RELINK_HOST}/r#${RELINK_HOST}`,
              qr: fakeQr(77),
              words: "river copper meadow thistle",
            },
            devices: [
              this.device("pixel", "Pixel", n - 12 * 24 * HOUR, today(7, 9)),
              this.device("work", "Work phone", n - 11 * 24 * HOUR, yesterday(18, 40), {
                needsAddress: true,
              }),
            ],
          },
          events,
        };
      case "problem":
        return {
          view: {
            ...on,
            state: { type: "problem", code: "helperFailed", attempts: 3, since: iso(today(7, 2)) },
            devices: [
              this.device("pixel", "Pixel", n - 12 * 24 * HOUR, today(6, 48)),
              this.device("work", "Work phone", n - 11 * 24 * HOUR, yesterday(18, 40)),
            ],
          },
          events,
        };
      case "noInternet":
        return {
          view: {
            ...base,
            state: { type: "problem", code: "noInternet", attempts: 1, since: iso(n) },
          },
          events: [],
        };
      default:
        return { view: base, events: [] };
    }
  }

  private event(
    deviceName: string | null,
    action: RemoteAction,
    at: number,
    result: RemoteResult = "ok",
    count = 1,
  ): RemoteEvent {
    return { deviceName, action, at: iso(at), result, count };
  }

  // Reading

  view(): RemoteView {
    return clone(this.v);
  }

  events(limit: number): RemoteEvent[] {
    return clone(this.log.slice(0, limit));
  }

  /** The Privacy screen's view of Away from home, and its ledger rows. */
  privacy(): Pick<
    PrivacyView,
    "sentToPhonesBytes" | "remoteOn" | "remoteConnection" | "remoteLastOnAt"
  > & { ledger: LedgerEntry[] } {
    const s = this.v.state.type;
    const live = s === "on" || s === "needsRelink" || s === "starting";
    const ledger: LedgerEntry[] = [];
    if (this.v.installed.length > 0 || this.helperBytes > 0) {
      const connection = this.v.installed.at(-1) ?? this.v.connection;
      ledger.push({
        kind: "remoteHelper",
        label: connection === "simple" ? "Cloudflare" : "Tailscale",
        bytes: this.helperBytes || DOWNLOAD_BYTES[connection],
        at: this.v.onSince ?? this.v.lastOnAt ?? iso(this.now()),
      });
    }
    if (this.v.lastOnAt) {
      ledger.push({ kind: "remoteOnOff", label: "on", bytes: 0, at: this.v.lastOnAt });
    }
    return {
      sentToPhonesBytes: this.v.sentToPhonesBytes,
      remoteOn: live,
      remoteConnection: this.v.setUpBefore || live ? this.v.connection : null,
      remoteLastOnAt: this.v.lastOnAt,
      ledger,
    };
  }

  advanced(): RemoteAdvanced | null {
    const s = this.v.state;
    if (s.type === "off" || s.type === "downloading") return null;
    const simple = this.v.connection === "simple";
    const latest = this.v.devices
      .filter((d) => d.lastUsedAt)
      .sort((a, b) => (b.lastUsedAt ?? "").localeCompare(a.lastUsedAt ?? ""))[0];
    return {
      connection: this.v.connection,
      helperName: simple ? "cloudflared" : "nocturne-tailnet",
      helperVersion: simple ? "2026.10.1" : "1.104.6",
      helperChecked: true,
      helperUptimeSeconds: 25_440,
      helperRestarts: 0,
      address: s.type === "on" || s.type === "needsRelink" ? s.address : null,
      addressSince: this.v.onSince,
      listener: "unix socket, owner only",
      route: simple
        ? "relayed by Cloudflare edge (LHR), WebSocket"
        : "direct, or relayed when a network blocks it",
      machineName: simple ? null : "nocturne-0000",
      lastContact: latest?.lastUsedAt
        ? { deviceName: latest.name, at: latest.lastUsedAt, sessionRenewedAt: null }
        : null,
      helperEstimateBytesPerDay: 1.8e6,
      auditEvents: this.log.length,
    };
  }

  // Changing

  private publish(): void {
    this.emit("remote-changed", this.view());
  }

  private set(over: Partial<RemoteView>): void {
    this.v = { ...this.v, ...over };
    this.publish();
  }

  private after(ms: number, fn: () => void): void {
    if (!this.auto) return;
    const timer = setTimeout(() => {
      this.timers.delete(timer);
      fn();
    }, ms);
    this.timers.add(timer);
  }

  private clearTimers(): void {
    for (const timer of this.timers) clearTimeout(timer);
    this.timers.clear();
  }

  private record(
    deviceName: string | null,
    action: RemoteAction,
    result: RemoteResult = "ok",
    count = 1,
  ): void {
    this.log.unshift(this.event(deviceName, action, this.now(), result, count));
  }

  /** The next scripted step of whatever is in progress. Timers call it; tests call it by hand. */
  advance(): void {
    const s = this.v.state;
    switch (s.type) {
      case "downloading": {
        if (s.paused) return;
        const received = Math.min(s.totalBytes, s.receivedBytes + s.totalBytes / 4);
        if (received >= s.totalBytes) {
          this.helperBytes = s.totalBytes;
          const installed = [...new Set([...this.v.installed, this.v.connection])];
          this.set({ installed, state: { type: "starting" } });
          this.after(this.stepMs, () => this.advance());
        } else {
          this.set({ state: { ...s, receivedBytes: received } });
          this.after(this.stepMs, () => this.advance());
        }
        return;
      }
      case "starting": {
        if (this.v.connection === "simple") {
          this.goOn(HOST, null);
        } else {
          this.set({
            state: { type: "needsSignIn", url: "https://login.example/a/3f9c", step: "signIn" },
          });
          this.after(this.stepMs * 2, () => this.advance());
        }
        return;
      }
      case "needsSignIn": {
        if (s.step === "signIn") {
          this.set({ state: { ...s, step: "allowLinks" } });
          this.after(this.stepMs * 2, () => this.advance());
        } else {
          this.goOn(TAILNET_HOST, "nocturne-0000");
        }
        return;
      }
      default:
        return;
    }
  }

  private goOn(address: string, machineName: string | null): void {
    const at = iso(this.now());
    this.record(null, "turnedOn");
    this.set({
      state: { type: "on", address, machineName },
      onSince: at,
      lastOnAt: at,
      setUpBefore: true,
    });
  }

  async start(connection: RemoteConnection, allowDownload: boolean): Promise<RemoteView> {
    const s = this.v.state;
    if (s.type === "downloading" && s.paused && allowDownload) {
      this.set({ state: { ...s, paused: false } });
      this.after(this.stepMs, () => this.advance());
      return this.view();
    }
    if (s.type !== "off" && s.type !== "problem") return this.view();
    this.clearTimers();
    if (!this.v.installed.includes(connection)) {
      if (!allowDownload)
        throw new EngineError("needsDownload", "The connection part is not installed");
      this.set({
        connection,
        state: {
          type: "downloading",
          receivedBytes: 0,
          totalBytes: DOWNLOAD_BYTES[connection],
          paused: false,
        },
      });
    } else {
      this.set({ connection, state: { type: "starting" } });
    }
    this.after(this.stepMs, () => this.advance());
    return this.view();
  }

  async stop(removeAll: boolean): Promise<RemoteView> {
    this.clearTimers();
    this.offer = null;
    const wasOn = this.v.state.type === "on" || this.v.state.type === "needsRelink";
    if (wasOn) this.record(null, "turnedOff");
    this.set({
      state: { type: "off" },
      pairing: null,
      onSince: null,
      devices: removeAll ? [] : this.v.devices,
    });
    return this.view();
  }

  async pauseDownload(): Promise<RemoteView> {
    const s = this.v.state;
    if (s.type === "downloading") this.set({ state: { ...s, paused: true } });
    return this.view();
  }

  async update(patch: RemoteSettingsPatch): Promise<RemoteView> {
    const over: Partial<RemoteView> = {};
    if (patch.autoOff) over.autoOff = patch.autoOff;
    if (patch.keepAwake !== null && patch.keepAwake !== undefined) over.keepAwake = patch.keepAwake;
    if (patch.connection && patch.connection !== this.v.connection) {
      this.clearTimers();
      this.offer = null;
      this.record(null, "switchedConnection");
      Object.assign(over, {
        connection: patch.connection,
        devices: [],
        state: { type: "off" } as RemoteState,
        pairing: null,
        onSince: null,
        sentToPhonesBytes: 0,
      });
    }
    this.set(over);
    return this.view();
  }

  async openSignIn(): Promise<void> {
    this.signInOpened += 1;
  }

  async newKey(): Promise<RemoteView> {
    this.keysMade += 1;
    this.clearTimers();
    this.offer = null;
    this.set({ devices: [], pairing: null });
    return this.view();
  }

  // Phones

  async beginPairing(): Promise<PairingOffer> {
    if (this.v.state.type !== "on")
      throw new EngineError("unavailable", "Away from home is not on");
    this.clearTimers();
    this.counter += 1;
    const pairingId = `pair-${this.counter}`;
    const expiresAt = iso(this.now() + OFFER_MS);
    const typedAddress = `${HOST}/p`;
    this.offer = {
      pairingId,
      link: `https://${HOST}/p#v1.sample.sample`,
      qr: fakeQr(10 + this.counter),
      typedAddress,
      // Spaced as the computer sends it.
      typedCode: "7KQ2M9 XDR4TB 0HJ5WC P8N3VA",
      expiresAt,
    };
    this.set({ pairing: { pairingId, phase: { type: "waitingForScan" }, expiresAt } });
    this.after(this.stepMs * 2, () => this.phoneArrives());
    this.after(OFFER_MS, () => this.expire());
    return clone(this.offer);
  }

  /** The scripted phone scans (or types) the code and asks to be added. */
  phoneArrives(): void {
    const status = this.v.pairing;
    if (status?.phase.type !== "waitingForScan") return;
    this.phoneNumber += 1;
    const request: PairingRequest = {
      pairingId: status.pairingId,
      deviceName: this.phoneNumber === 1 ? "Pixel" : `Phone ${this.phoneNumber}`,
      checkPhrase: WORDS,
      decideBy: iso(this.now() + DECIDE_MS),
    };
    this.setPhase({ type: "waitingForYou", request });
    this.emit("pairing-request", clone(request));
    this.after(DECIDE_MS, () => this.timeOut());
  }

  /** Which way the scripted phone pairs, for the screen's own words. */
  get phonePath(): "scanned" | "typed" {
    return this.phone;
  }

  private setPhase(phase: PairingPhase): void {
    const status = this.v.pairing;
    if (!status) return;
    this.set({ pairing: { ...status, phase } });
  }

  timeOut(): void {
    if (this.v.pairing?.phase.type !== "waitingForYou") return;
    this.record(null, "refused", "refused");
    this.setPhase({ type: "timedOut" });
  }

  expire(): void {
    const t = this.v.pairing?.phase.type;
    if (t !== "waitingForScan") return;
    this.offer = null;
    this.setPhase({ type: "expired" });
  }

  async decide(pairingId: string, allow: boolean): Promise<void> {
    const status = this.v.pairing;
    if (!status || status.pairingId !== pairingId || status.phase.type !== "waitingForYou") {
      throw new EngineError("unavailable", "Nothing is waiting for a decision");
    }
    const name = status.phase.request.deviceName;
    this.clearTimers();
    if (!allow) {
      this.record(null, "refused", "refused");
      this.setPhase({ type: "refused" });
      return;
    }
    const at = this.now();
    this.record(name, "allowed");
    this.setPhase({ type: "allowed", deviceName: name });
    this.set({
      devices: [
        ...this.v.devices,
        this.device(`phone-${this.counter}`, name, at, at, { active: true }),
      ],
    });
  }

  async cancelPairing(): Promise<void> {
    this.clearTimers();
    this.offer = null;
    if (this.v.pairing) this.set({ pairing: null });
  }

  async rename(id: string, name: string): Promise<DeviceView> {
    const found = this.v.devices.find((d) => d.id === id);
    if (!found) throw new EngineError("notFound", "No such phone");
    const clean = name.trim();
    if (clean === "" || new TextEncoder().encode(clean).length > 40) {
      throw new EngineError("invalid", "A phone's name is 1 to 40 characters");
    }
    const renamed = { ...found, name: clean };
    this.record(clean, "renamed");
    this.set({ devices: this.v.devices.map((d) => (d.id === id ? renamed : d)) });
    return clone(renamed);
  }

  async remove(id: string): Promise<void> {
    const found = this.v.devices.find((d) => d.id === id);
    if (!found) throw new EngineError("notFound", "No such phone");
    this.record(found.name, "removed");
    this.set({ devices: this.v.devices.filter((d) => d.id !== id) });
  }

  // Scripted events a test or a design review can cause

  /** A phone starts or stops using Nocturne. */
  setActive(id: string, active: boolean): void {
    const found = this.v.devices.find((d) => d.id === id);
    if (!found) return;
    const next = { ...found, active, lastUsedAt: iso(this.now()) };
    this.v = { ...this.v, devices: this.v.devices.map((d) => (d.id === id ? next : d)) };
    this.emit("device-active", clone(next));
    this.publish();
  }

  /** The computer restarted: Simple has a new address, and every phone needs it. */
  restartSimple(): void {
    if (this.v.connection !== "simple" || this.v.state.type !== "on") return;
    this.set({
      state: {
        type: "needsRelink",
        address: RELINK_HOST,
        pairingLink: `https://${RELINK_HOST}/r#${RELINK_HOST}`,
        qr: fakeQr(77),
        words: "river copper meadow thistle",
      },
      devices: this.v.devices.map((d) => ({ ...d, needsAddress: true, active: false })),
    });
  }

  /** A paired phone scanned or typed the new address. */
  phoneRelinks(id: string): void {
    const devices = this.v.devices.map((d) =>
      d.id === id ? { ...d, needsAddress: false, lastUsedAt: iso(this.now()) } : d,
    );
    const found = this.v.devices.find((d) => d.id === id);
    if (found) this.record(found.name, "tookNewAddress");
    const done = devices.every((d) => !d.needsAddress);
    this.set({
      devices,
      state: done ? { type: "on", address: RELINK_HOST, machineName: null } : this.v.state,
    });
  }

  /** The connection could not start or stay up. */
  fail(code: RemoteProblem = "helperFailed", attempts = 3): void {
    this.clearTimers();
    this.set({ state: { type: "problem", code, attempts, since: iso(this.now()) } });
  }

  /** Some bytes left for the phones. */
  sent(bytes: number): void {
    this.set({ sentToPhonesBytes: this.v.sentToPhonesBytes + bytes });
  }

  /** Auto-off came due: it stops as the switch does, and the phones stay paired. */
  autoOffDue(): void {
    void this.stop(false);
  }

  dispose(): void {
    this.clearTimers();
  }
}
