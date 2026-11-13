// The computer's side of the sealed channel, in WebCrypto, for tests and for the phone app's
// own mock build. It speaks the same frames as the real listener (plan 12 "Sealed channel") over
// two shims, a WebSocket and a long-poll `fetch`, so the phone's client runs against it with no
// network. It never runs in the shipped phone app.

import type {
  RemoteAnswer,
  RemoteConnection,
  RemoteReply,
  RemoteRequest,
} from "../../engine/bindings";
import {
  type Bytes,
  concat,
  copy,
  equal,
  fromBase64Url,
  fromUtf8,
  readU32,
  toBase64Url,
  toHex,
  u32,
  utf8,
} from "../bytes";
import type { Env, WebSocketLike } from "../channel";
import * as C from "../crypto";
import * as P from "../protocol";

export type Outcome = "ok" | "refuse" | "gone" | "notFound";

export interface Sink {
  send(frame: Bytes): void;
}

export interface FakeDevice {
  id: Bytes;
  name: string;
  dpub: Bytes;
  revoked: boolean;
  path: number;
}

export interface Offer {
  s: Bytes;
  code: string;
  pid: Bytes;
  link: string;
  expiresAt: number;
  committed: boolean;
  spent: boolean;
  nc: Bytes | null;
  nonceAt: number;
  keys: { up: Bytes; down: Bytes };
  sink: Sink | null;
  commit: { path: number; deviceName: string; commit: Bytes } | null;
  decide: ((allow: boolean) => Promise<void>) | null;
  words: string[] | null;
}

export interface FakeSession {
  sid: Bytes;
  device: FakeDevice;
  keys: { up: Bytes; down: Bytes };
  upLast: number;
  downLast: number;
  sink: Sink;
  closed: string | null;
  live: boolean;
}

export interface CallContext {
  device: FakeDevice;
  session: FakeSession;
  request: RemoteRequest;
  reply(answer: RemoteAnswer): Promise<void>;
  chunk(seq: number, last: boolean, data: Uint8Array): Promise<void>;
}

export type Handler = (ctx: CallContext) => void | Promise<void>;

export interface Upload {
  callId: number;
  size: number;
  mime: string;
  clientId: string;
  caption: string | null;
  chunks: Bytes[];
  received: number;
}

class PollQueue implements Sink {
  frames: Bytes[] = [];
  /** A held poll is waiting on this queue. */
  held = false;
  private waiter: (() => void) | null = null;
  send(frame: Bytes): void {
    this.frames.push(frame);
    this.wake();
  }
  wake(): void {
    const w = this.waiter;
    this.waiter = null;
    w?.();
  }
  async take(holdMs: number, signal: AbortSignal | null | undefined): Promise<Bytes[]> {
    if (this.frames.length === 0) {
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, holdMs);
        this.waiter = () => {
          clearTimeout(timer);
          resolve();
        };
        signal?.addEventListener("abort", () => {
          clearTimeout(timer);
          resolve();
        });
      });
    }
    return this.frames.splice(0, P.MAX_LONG_POLL_BATCH);
  }
}

export interface ComputerOptions {
  host?: string;
  connection?: RemoteConnection;
  version?: string;
  computerName?: string;
  minPhoneBuild?: number;
  cKey?: { privateKey: CryptoKey; publicKey: Bytes };
  now?: () => number;
}

/** The refusals the open step gives; mirrors the reference's `verify_open`. */
export async function verifyOpen(
  frame: Uint8Array,
  registry: Map<string, Bytes>,
  cPrivate: CryptoKey,
): Promise<{ deviceId: Bytes; epub: Bytes; on: Bytes }> {
  const h = C.parseHeader(frame);
  const need =
    P.HEADER_LEN +
    P.DEVICE_ID_LEN +
    P.PUBKEY_LEN +
    P.OPEN_NONCE_LEN +
    P.PROOF_OPEN.length +
    P.GCM_TAG_LEN;
  if (frame.length !== need) throw new C.Refusal("tooShort");
  if (h.version !== P.VERSION) throw new C.Refusal("badVersion");
  if (h.kind !== P.KIND_SESSION_OPEN) throw new C.Refusal("wrongKind");
  if (!equal(h.sid, C.zeroSid())) throw new C.Refusal("wrongSid");
  if (h.ctr !== 0) throw new C.Refusal("badCounter");
  let at = P.HEADER_LEN;
  const deviceId = copy(frame.subarray(at, at + P.DEVICE_ID_LEN));
  at += P.DEVICE_ID_LEN;
  const epub = copy(frame.subarray(at, at + P.PUBKEY_LEN));
  at += P.PUBKEY_LEN;
  const on = copy(frame.subarray(at, at + P.OPEN_NONCE_LEN));
  at += P.OPEN_NONCE_LEN;
  const dpub = registry.get(toHex(deviceId));
  if (!dpub) throw new C.Refusal("badProof");
  const kd = await C.deviceKey(await C.ecdh(cPrivate, dpub), deviceId);
  const aad = frame.subarray(0, P.HEADER_LEN + P.DEVICE_ID_LEN + P.PUBKEY_LEN);
  const key = await C.aesKey(kd, "decrypt");
  let plain: Uint8Array;
  try {
    plain = new Uint8Array(
      await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: copy(on), additionalData: copy(aad), tagLength: 128 },
        key,
        copy(frame.subarray(at)),
      ),
    );
  } catch {
    throw new C.Refusal("badProof");
  }
  if (!equal(plain, utf8(P.PROOF_OPEN))) throw new C.Refusal("badProof");
  C.checkPublicKey(epub);
  return { deviceId, epub, on };
}

const ask = (id: number, answer: RemoteAnswer): RemoteReply => ({ id, answer });

export class FakeComputer {
  readonly host: string;
  readonly connection: RemoteConnection;
  readonly version: string;
  readonly computerName: string;
  readonly minPhoneBuild: number | undefined;
  readonly cpub: Bytes;
  readonly devices = new Map<string, FakeDevice>();
  readonly sessions = new Map<string, FakeSession>();
  readonly offers = new Map<string, Offer>();
  /** Every frame seen, in order: what a relay would see. */
  readonly seen: { up: Bytes[]; down: Bytes[] } = { up: [], down: [] };
  readonly uploads = new Map<string, Upload>();
  readonly items = new Map<
    string,
    { clientId: string; text: string | null; mime: string | null; bytes: Bytes | null }
  >();
  opens = 0;
  refusals: string[] = [];
  /** Set to make every request go to this instead of the default. */
  handler: Handler | null = null;
  /** How a pairing is decided when no test is waiting on it. */
  autoDecide: "allow" | "refuse" | null = null;
  /** The phone's own view of pairing, as the computer sees it. */
  lastWords: string[] | null = null;
  wsAvailable = true;
  networkDown = false;
  /** Away from home is off: every request is a bare 503. */
  off = false;
  httpStatus: number | null = null;
  holdMs = 50;
  /** Rewrites the frames the computer sends in a session: replay, reorder, drop or tamper. */
  mutate: ((frame: Bytes, ctr: number) => Bytes[]) | null = null;
  /** How many sockets close, unanswered, when their first frame arrives. */
  dropSocketOpens = 0;
  /** Answer every open with an unsealed refusal. */
  refuseOpens = false;
  /** A REVOKED sealed for a different open than the phone's. */
  revokedForOtherOpen = false;
  now: () => number;

  private readonly cPrivate: CryptoKey;
  private readonly queues = new Map<string, PollQueue>();
  private readonly sockets = new Set<FakeSocket>();
  private readonly aborts = new Set<AbortController>();
  private chain: Promise<unknown> = Promise.resolve();
  /** Conversations whose queued frames ride back on the POST that started them. */
  private drain = new Set<string>();
  /** Frames that answer the POST in hand and have no conversation to wait under. */
  private replyNow: Bytes[] = [];

  private constructor(cKey: { privateKey: CryptoKey; publicKey: Bytes }, o: ComputerOptions) {
    this.cPrivate = cKey.privateKey;
    this.cpub = cKey.publicKey;
    this.host = o.host ?? "amber-otter-lamp-quiet.trycloudflare.com";
    this.connection = o.connection ?? "simple";
    this.version = o.version ?? "0.1.0";
    this.computerName = o.computerName ?? "Home computer";
    this.minPhoneBuild = o.minPhoneBuild;
    this.now = o.now ?? (() => Date.now());
  }

  static async create(o: ComputerOptions = {}): Promise<FakeComputer> {
    const key = o.cKey ?? (await C.generateDeviceKey());
    return new FakeComputer(key, o);
  }

  // The network the phone sees ------------------------------------------------------------------

  get env(): Env {
    const self = this;
    class Socket extends FakeSocket {
      constructor(url: string) {
        super(self, url);
      }
    }
    return {
      fetch: (input, init) => this.fetch(String(input), init),
      WebSocket: Socket,
      now: () => this.now(),
    };
  }

  /** A transport error: every socket closes and every held poll ends. */
  dropAll(): void {
    for (const s of [...this.sockets]) s.serverClose();
    for (const a of this.aborts) a.abort();
    for (const q of this.queues.values()) q.wake();
  }

  /** The computer ends a session: a held poll for it answers 410. */
  endSession(sid: Bytes): void {
    const session = this.sessions.get(toHex(sid));
    if (session) session.closed = "ended";
    this.queues.get(toHex(sid))?.wake();
  }

  private serial<T>(fn: () => Promise<T>): Promise<T> {
    const run = this.chain.then(fn, fn);
    this.chain = run.catch(() => undefined);
    return run;
  }

  private queue(sid: Bytes): PollQueue {
    const key = toHex(sid);
    let q = this.queues.get(key);
    if (!q) {
      q = new PollQueue();
      this.queues.set(key, q);
    }
    return q;
  }

  /** Whether a conversation by this id is there to poll: a live session or a pairing. */
  private known(sid: string): boolean {
    const session = this.sessions.get(sid);
    if (session) return session.closed === null;
    return this.offers.get(sid)?.committed === true;
  }

  /** The listener's long-poll contract: see `listener.rs` in the computer's remote module. */
  async fetch(input: string, init?: RequestInit): Promise<Response> {
    if (this.networkDown) throw new TypeError("network down");
    if (this.httpStatus !== null) return new Response(null, { status: this.httpStatus });
    // While Away from home is off every keyed request is a bare 503.
    if (this.off) return new Response(null, { status: 503 });
    const url = new URL(input);
    if (url.pathname !== P.PATH_SESSION) return new Response(null, { status: 404 });
    const signal = init?.signal ?? null;
    if (init?.method === "GET") {
      const sid = url.searchParams.get("sid") ?? "";
      if (!/^[0-9a-f]{32}$/.test(sid)) return new Response(null, { status: 404 });
      if (!this.known(sid)) return new Response(null, { status: 410 });
      const q = this.queue(hexBytes(sid));
      if (q.held) return new Response(null, { status: 409 });
      const ac = new AbortController();
      this.aborts.add(ac);
      signal?.addEventListener("abort", () => ac.abort());
      q.held = true;
      try {
        const frames = await q.take(this.holdMs, ac.signal);
        if (signal?.aborted || ac.signal.aborted) throw new DOMException("aborted", "AbortError");
        if (frames.length === 0) {
          return new Response(null, { status: this.known(sid) ? 204 : 410 });
        }
        return new Response(frameRun(frames), { status: 200 });
      } finally {
        q.held = false;
        this.aborts.delete(ac);
      }
    }
    if (init?.method !== "POST") return new Response(null, { status: 404 });
    const body = new Uint8Array(await new Response(init?.body as BodyInit).arrayBuffer());
    const frames = splitRun(body);
    if (!frames || frames.length > P.MAX_LONG_POLL_BATCH)
      return new Response(null, { status: 413 });
    this.drain = new Set();
    this.replyNow = [];
    for (const f of frames) {
      const outcome = await this.receive(f, null);
      if (outcome === "refuse") return new Response(null, { status: 403 });
      if (outcome === "gone") return new Response(null, { status: 410 });
      if (outcome === "notFound") return new Response(null, { status: 404 });
    }
    // Only the conversations this POST began or answered hand back their frames with it.
    const reply = [...this.replyNow];
    for (const sid of this.drain) {
      const q = this.queues.get(sid);
      if (q) reply.push(...q.frames.splice(0, P.MAX_LONG_POLL_BATCH - reply.length));
    }
    return reply.length
      ? new Response(frameRun(reply), { status: 200 })
      : new Response(null, { status: 204 });
  }

  // Offers and pairing --------------------------------------------------------------------------

  async offer(path: "scanned" | "typed" = "scanned"): Promise<Offer> {
    const s = C.randomBytes(P.S_LEN);
    const pid = await C.pairingId(s);
    const keys = await C.pairKeys(s, this.cpub, pid);
    const offer: Offer = {
      s,
      code: C.codeEncode(s),
      pid,
      link: C.pairingLink(this.host, s, this.cpub),
      expiresAt: this.now() + P.PAIR_TTL_SECS * 1000,
      committed: false,
      spent: false,
      nc: null,
      nonceAt: 0,
      keys: { up: keys.up, down: keys.down },
      sink: null,
      commit: null,
      decide: null,
      words: null,
    };
    void path;
    this.offers.set(toHex(pid), offer);
    return offer;
  }

  /** The person's decision on the offer's pending request. */
  async decide(offer: Offer, allow: boolean): Promise<void> {
    await offer.decide?.(allow);
  }

  private openOffer(pid: Bytes): Offer | null {
    const offer = this.offers.get(toHex(pid));
    if (!offer || offer.spent || this.now() > offer.expiresAt) return null;
    return offer;
  }

  // Sessions ------------------------------------------------------------------------------------

  /** The computer removes a phone: later opens get a sealed REVOKED, live sessions end sealed. */
  async revoke(deviceId: Bytes): Promise<void> {
    const device = this.devices.get(toHex(deviceId));
    if (!device) return;
    device.revoked = true;
    for (const s of this.sessions.values()) {
      if (s.device === device && s.closed === null) {
        await this.sealDown(s, ask(0, { type: "revoked" }));
        s.closed = "revoked";
      }
    }
  }

  private async sealDown(session: FakeSession, reply: RemoteReply): Promise<void> {
    const env = C.envelopeJson(utf8(JSON.stringify(reply)));
    await this.sendDown(session, env);
  }

  private async sendDown(session: FakeSession, env: Bytes): Promise<void> {
    const ctr = session.downLast + 1;
    const frame = await C.sealDataFrame(session.keys.down, "down", session.sid, ctr, env);
    session.downLast = ctr;
    this.seen.down.push(frame);
    for (const f of this.mutate ? this.mutate(frame, ctr) : [frame]) session.sink.send(f);
  }

  // The frames ----------------------------------------------------------------------------------

  /**
   * One frame from a phone. `via` is a WebSocket's sink; null means long-poll, where frames for
   * a conversation wait in its queue. Calls are handled one at a time, in order.
   */
  receive(frame: Bytes, via: Sink | null): Promise<Outcome> {
    return this.serial(() => this.handle(frame, via));
  }

  /** A sink that puts frames in a conversation's long-poll queue. */
  private pollSink(sid: Bytes): Sink {
    const queue = this.queue(sid);
    return { send: (f) => queue.send(f) };
  }

  private async handle(frame: Bytes, via: Sink | null): Promise<Outcome> {
    this.seen.up.push(frame);
    let h: C.Header;
    try {
      h = C.parseHeader(frame);
    } catch {
      return "notFound";
    }
    if (h.version !== P.VERSION) return "notFound";
    switch (h.kind) {
      case P.KIND_HELLO: {
        const offer = this.openOffer(h.sid);
        // No open offer: nothing answers, and nothing is kept.
        if (!offer || frame.length !== P.HEADER_LEN) return "ok";
        this.drain.add(toHex(h.sid));
        (via ?? this.pollSink(h.sid)).send(concat(frame.subarray(0, P.HEADER_LEN), this.cpub));
        return "ok";
      }
      case P.KIND_PAIR_COMMIT:
      case P.KIND_PAIR_REVEAL:
        this.drain.add(toHex(h.sid));
        await this.pairing(h, frame, via ?? this.pollSink(h.sid));
        // A junk or out-of-turn pairing frame is not answered and ends nothing.
        return "ok";
      case P.KIND_SESSION_OPEN:
        return this.sessionOpen(frame, via);
      case P.KIND_DATA:
        return this.data(h, frame);
      default:
        return "notFound";
    }
  }

  private async pairing(h: C.Header, frame: Bytes, sink: Sink): Promise<boolean> {
    const offer = this.openOffer(h.sid);
    if (!offer) return false;
    try {
      const opened = await C.openPairFrame(frame, h.kind, offer.pid, offer.keys.up);
      const body = JSON.parse(fromUtf8(opened.body)) as Record<string, string | number>;
      if (h.kind === P.KIND_PAIR_COMMIT) {
        if (offer.committed) {
          offer.spent = true;
          return false;
        }
        offer.committed = true;
        offer.sink = sink;
        const name = String(body.deviceName);
        C.checkDeviceName(name);
        offer.commit = {
          path: Number(body.path),
          deviceName: name,
          commit: fromBase64Url(String(body.commit)),
        };
        offer.nc = C.randomBytes(P.NONCE_PAIR_LEN);
        offer.nonceAt = this.now();
        await this.sealPair(offer, sink, P.KIND_PAIR_NONCE, { nc: toBase64Url(offer.nc) });
        return true;
      }
      // reveal
      if (!offer.commit || !offer.nc || this.now() - offer.nonceAt > P.REVEAL_WINDOW_SECS * 1000) {
        offer.spent = true;
        return false;
      }
      const dpub = fromBase64Url(String(body.dpub));
      const np = fromBase64Url(String(body.np));
      const nameRaw = C.checkDeviceName(offer.commit.deviceName);
      await C.checkReveal(this.cpub, dpub, np, nameRaw, offer.commit.commit);
      const shared = await C.ecdh(this.cPrivate, dpub);
      const salt = await C.wordsSalt({
        path: offer.commit.path,
        pairingId: offer.pid,
        host: this.host,
        cpub: this.cpub,
        dpub,
        nameRaw,
        np,
        nc: offer.nc,
      });
      const indices = await C.wordIndices(shared, salt);
      offer.words = indices.map((i) => P.WORDS[i] ?? "");
      this.lastWords = offer.words;
      offer.decide = async (allow) => {
        offer.decide = null;
        if (!allow) {
          await this.sealPair(offer, offer.sink ?? sink, P.KIND_PAIR_OUTCOME, { allowed: false });
          offer.spent = true;
          return;
        }
        const device: FakeDevice = {
          id: C.randomBytes(P.DEVICE_ID_LEN),
          name: offer.commit?.deviceName ?? "",
          dpub,
          revoked: false,
          path: offer.commit?.path ?? 1,
        };
        this.devices.set(toHex(device.id), device);
        await this.sealPair(offer, offer.sink ?? sink, P.KIND_PAIR_OUTCOME, {
          allowed: true,
          deviceId: toBase64Url(device.id),
        });
        offer.spent = true;
      };
      if (this.autoDecide) {
        const allow = this.autoDecide === "allow";
        void this.serial(() => offer.decide?.(allow) ?? Promise.resolve());
      }
      return true;
    } catch {
      this.refusals.push("pairing");
      return false;
    }
  }

  private async sealPair(offer: Offer, sink: Sink, kind: number, body: object): Promise<void> {
    const env = C.envelopeJson(utf8(JSON.stringify(body)));
    const frame = await C.sealPairFrame(kind, offer.pid, offer.keys.down, C.randomBytes(12), env);
    this.seen.down.push(frame);
    sink.send(frame);
  }

  private async sessionOpen(frame: Bytes, via: Sink | null): Promise<Outcome> {
    this.opens += 1;
    if (this.refuseOpens) return "refuse";
    const registry = new Map<string, Bytes>(
      [...this.devices].map(([id, d]) => [id, d.dpub] as [string, Bytes]),
    );
    let opened: Awaited<ReturnType<typeof verifyOpen>>;
    try {
      opened = await verifyOpen(frame, registry, this.cPrivate);
    } catch (e) {
      this.refusals.push(e instanceof C.Refusal ? e.code : "open");
      return "refuse";
    }
    const device = this.devices.get(toHex(opened.deviceId));
    if (!device) return "refuse";
    if (device.revoked) {
      const kd = await C.deviceKey(await C.ecdh(this.cPrivate, device.dpub), device.id);
      const nonce = this.revokedForOtherOpen ? C.randomBytes(P.OPEN_NONCE_LEN) : opened.on;
      const rev = await C.sealRevoked(kd, device.id, nonce, C.randomBytes(12));
      this.seen.down.push(rev);
      // Poll: a REVOKED has no session to wait under, so it goes back on the POST itself.
      if (via) via.send(rev);
      else this.replyNow.push(rev);
      return "ok";
    }
    const f = await C.generateSessionKey();
    const sid = C.randomBytes(P.SID_LEN);
    const terms: C.SessionTerms = {
      deviceId: device.id,
      epub: opened.epub,
      openNonce: opened.on,
      sid,
      fpub: f.publicKey,
      cpub: this.cpub,
      dpub: device.dpub,
    };
    const ikm = await Promise.all([
      C.ecdh(f.privateKey, opened.epub),
      C.ecdh(this.cPrivate, opened.epub),
      C.ecdh(f.privateKey, device.dpub),
      C.ecdh(this.cPrivate, device.dpub),
    ]);
    const keys = await C.sessionKeyBytes(terms, ikm as [Bytes, Bytes, Bytes, Bytes]);
    const sink = via ?? this.pollSink(sid);
    const session: FakeSession = {
      sid,
      device,
      keys,
      upLast: 0,
      downLast: 0,
      sink,
      closed: null,
      live: false,
    };
    this.sessions.set(toHex(sid), session);
    this.drain.add(toHex(sid));
    const accept = C.sealSessionAccept(sid, f.publicKey);
    this.seen.down.push(accept);
    sink.send(accept);
    await this.sealDown(
      session,
      ask(0, {
        type: "hello",
        hello: {
          computerName: this.computerName,
          deviceName: device.name,
          connection: this.connection,
          version: this.version,
          minPhoneBuild: this.minPhoneBuild ?? P.MIN_PHONE_BUILD,
        },
      }),
    );
    return "ok";
  }

  private async data(h: C.Header, frame: Bytes): Promise<Outcome> {
    const session = this.sessions.get(toHex(h.sid));
    // A session that is not there, or has ended, is gone (410 over long-poll).
    if (!session || session.closed !== null) return "gone";
    try {
      const env = await C.openDataFrame(frame, session.keys.up, "up", session.sid, session.upLast);
      session.upLast = h.ctr;
      session.live = true;
      if (session.device.revoked) return "ok";
      if (env.type === P.TYPE_CHUNK) {
        await this.onChunk(session, C.chunkParse(env.body));
        return "ok";
      }
      const request = JSON.parse(fromUtf8(env.body)) as RemoteRequest;
      await this.dispatch(session, request);
      return "ok";
    } catch (e) {
      session.closed = e instanceof C.Refusal ? e.code : "bad frame";
      this.refusals.push(session.closed);
      if (session.sink instanceof FakeSocketSink) session.sink.close();
      return "ok";
    }
  }

  private async onChunk(session: FakeSession, chunk: C.Chunk): Promise<void> {
    const key = `${toHex(session.sid)}:${chunk.callId}`;
    const up = this.uploads.get(key);
    if (!up) return;
    up.chunks.push(chunk.data);
    up.received += chunk.data.length;
    if (chunk.last) {
      const total = concat(...up.chunks);
      const id = `item-${this.items.size + 1}`;
      const kept = [...this.items.values()].find((i) => i.clientId === up.clientId);
      if (!kept)
        this.items.set(id, { clientId: up.clientId, text: null, mime: up.mime, bytes: total });
      const shown = kept ? ([...this.items].find(([, v]) => v === kept)?.[0] ?? id) : id;
      await this.sealDown(
        session,
        ask(up.callId, keptAnswer(shown, up.caption ?? up.mime, !!kept, this.now())),
      );
    }
  }

  private async dispatch(session: FakeSession, request: RemoteRequest): Promise<void> {
    const ctx: CallContext = {
      device: session.device,
      session,
      request,
      reply: (answer) => this.sealDown(session, ask(request.id, answer)),
      chunk: (seq, last, data) =>
        this.sendDown(session, C.envelopeChunk(request.id, seq, last, data)),
    };
    if (this.handler) return void (await this.handler(ctx));
    await this.defaultHandler(ctx);
  }

  async defaultHandler(ctx: CallContext): Promise<void> {
    const call = ctx.request.call;
    const id = ctx.request.id;
    switch (call.cmd) {
      case "getStatus":
        return ctx.reply({
          type: "status",
          status: { readiness: "ready", pageDate: "2026-11-13", needsYou: 2 },
        });
      case "keepText": {
        const existing = [...this.items].find(([, v]) => v.clientId === call.clientId);
        const itemId = existing?.[0] ?? `item-${this.items.size + 1}`;
        if (!existing)
          this.items.set(itemId, {
            clientId: call.clientId,
            text: call.text,
            mime: null,
            bytes: null,
          });
        return ctx.reply(keptAnswer(itemId, call.text.slice(0, 40), !!existing, this.now()));
      }
      case "keepBytes":
      case "keepRecording":
        this.uploads.set(`${toHex(ctx.session.sid)}:${id}`, {
          callId: id,
          size: call.size,
          mime: call.mime,
          clientId: call.clientId,
          caption: call.cmd === "keepBytes" ? call.caption : null,
          chunks: [],
          received: 0,
        });
        return;
      case "unkeep":
        return ctx.reply({ type: "done" });
      case "ask":
        await ctx.reply({ type: "askStarted", askId: "ask-1" });
        for (const [phase, text] of [
          ["searching", null],
          ["streaming", "Swimming is on "],
          ["streaming", "**Monday**."],
          ["done", null],
        ] as const) {
          await ctx.reply({ type: "askDelta", delta: delta("ask-1", phase, text) });
        }
        return;
      case "cancelAsk":
      case "disconnect":
        return ctx.reply({ type: "done" });
      case "listPages":
        return ctx.reply({
          type: "pages",
          pages: [{ date: "2026-11-12", pageNumber: 41, headline: "Quiet day.", needsYou: 0 }],
        });
      default:
        return ctx.reply({ type: "error", code: "invalid" });
    }
  }
}

export function delta(
  askId: string,
  phase:
    | "waking"
    | "searching"
    | "reading"
    | "streaming"
    | "done"
    | "notSure"
    | "nothing"
    | "unavailable",
  textDelta: string | null,
) {
  return {
    askId,
    phase,
    textDelta,
    receipts: [],
    memoryIds: [],
    near: [],
    found: null,
    etaSeconds: null,
    plain: false,
  };
}

function keptAnswer(id: string, title: string, duplicate: boolean, now: number): RemoteAnswer {
  return {
    type: "kept",
    kept: {
      item: {
        id,
        kind: "text",
        title,
        keptAt: new Date(now).toISOString(),
        durationMs: null,
        pages: null,
        hasOriginal: false,
        summary: null,
        text: null,
        truncated: false,
      },
      duplicate,
      takeBackUntil: new Date(now + P.TAKE_BACK_SECS * 1000).toISOString(),
    },
  };
}

function hexBytes(hex: string): Bytes {
  return copy(Uint8Array.from(hex.match(/../g) ?? [], (h) => Number.parseInt(h, 16)));
}

function frameRun(frames: Bytes[]): Bytes {
  return concat(...frames.flatMap((f) => [u32(f.length), f]));
}

function splitRun(body: Uint8Array): Bytes[] | null {
  const frames: Bytes[] = [];
  let at = 0;
  while (at < body.length) {
    if (at + 4 > body.length) return null;
    const n = readU32(body, at);
    at += 4;
    if (at + n > body.length) return null;
    frames.push(copy(body.subarray(at, at + n)));
    at += n;
  }
  return frames;
}

class FakeSocketSink implements Sink {
  readonly socket = true;
  constructor(private readonly s: FakeSocket) {}
  send(frame: Bytes): void {
    this.s.toPhone(frame);
  }
  close(): void {
    this.s.serverClose();
  }
}

/** The phone's WebSocket, wired straight to the computer. */
export class FakeSocket implements WebSocketLike {
  binaryType = "blob";
  bufferedAmount = 0;
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  private closed = false;
  private readonly sink = new FakeSocketSink(this);

  constructor(
    private readonly computer: FakeComputer,
    readonly url: string,
  ) {
    const sockets = (computer as unknown as { sockets: Set<FakeSocket> }).sockets;
    queueMicrotask(() => {
      if (!computer.wsAvailable || computer.networkDown || computer.off) {
        this.closed = true;
        this.onerror?.({});
        this.onclose?.({});
        return;
      }
      sockets.add(this);
      this.onopen?.({});
    });
  }

  send(data: Uint8Array): void {
    if (this.closed) throw new Error("socket closed");
    if (this.computer.dropSocketOpens > 0) {
      this.computer.dropSocketOpens -= 1;
      this.serverClose(1006);
      return;
    }
    const frame = copy(data);
    void this.computer.receive(frame, this.sink).then((outcome) => {
      // The unsealed refusal is a close with 4403; a session that is gone is an ordinary close.
      if (outcome === "refuse") this.serverClose(4403);
      else if (outcome !== "ok") this.serverClose();
    });
  }

  toPhone(frame: Bytes): void {
    if (this.closed) return;
    queueMicrotask(() => this.onmessage?.({ data: frame.buffer.slice(0) }));
  }

  serverClose(code = 1000): void {
    if (this.closed) return;
    this.closed = true;
    (this.computer as unknown as { sockets: Set<FakeSocket> }).sockets.delete(this);
    queueMicrotask(() => this.onclose?.({ code }));
  }

  close(): void {
    this.serverClose();
  }
}
