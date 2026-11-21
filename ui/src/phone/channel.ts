// The phone's sealed channel to the computer: a link (WebSocket, or long-poll when the socket
// will not open), the handshake, and a session that numbers, seals and opens frames.
//
// Nothing here is stored. Session keys, counters and ids live in this module's objects and go
// with the tab. A frame is never sent twice: after any transport error the session is dead and
// the caller opens a new one and retries the call above this layer.
//
// Long-poll wire form (the computer's listener serves the same): every body is a run of frames,
// each as a 4-byte big-endian length then the frame. `POST /s` carries up to
// `MAX_LONG_POLL_BATCH` frames up and answers with any frames the computer has ready; the body
// is labelled text/plain so a cross-origin POST needs no preflight. `GET /s?sid=<32 hex>` is held
// up to `LONG_POLL_HOLD_SECS` and answers 200 with frames or 204 for none. There is at most one
// of each in flight.

import type { RemoteAnswer, RemoteCall, RemoteReply } from "../engine/bindings";
import { type Bytes, concat, copy, fromUtf8, readU32, toHex, u32, utf8 } from "./bytes";
import {
  type Chunk,
  chunkParse,
  deviceKey,
  ecdh,
  envelopeChunk,
  envelopeJson,
  generateSessionKey,
  openDataFrame,
  openRevoked,
  openSessionAccept,
  phoneSessionKeys,
  Refusal,
  randomBytes,
  type SessionKeys,
  sealDataFrame,
  sealSessionOpen,
} from "./crypto";
import type { PairedDevice } from "./engine";
import { PhoneRequestError } from "./engine";
import * as P from "./protocol";
import { HTTP_ENDED, HTTP_OFF, HTTP_POLL_BUSY, HTTP_REFUSED } from "./protocol";
import { WS_REFUSED } from "./status";

// Environment -------------------------------------------------------------------------------------

/** The part of a WebSocket the channel uses; the real one and the test one both fit. */
export interface WebSocketLike {
  binaryType: string;
  readonly bufferedAmount: number;
  send(data: Uint8Array): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

export interface Env {
  fetch: typeof fetch;
  WebSocket: (new (url: string) => WebSocketLike) | undefined;
  now(): number;
}

export function defaultEnv(): Env {
  return {
    fetch: (input, init) => globalThis.fetch(input, init),
    WebSocket:
      typeof globalThis.WebSocket === "function"
        ? (globalThis.WebSocket as unknown as new (
            url: string,
          ) => WebSocketLike)
        : undefined,
    now: () => Date.now(),
  };
}

/** `https://host` from a host, or from an origin that already says its scheme. */
export function originOf(address: string): string {
  return address.includes("://") ? address.replace(/\/+$/, "") : `https://${address}`;
}

// Links -------------------------------------------------------------------------------------------

export type LinkEnd =
  | { kind: "closed"; opened: boolean; code: number }
  | { kind: "network" }
  | { kind: "http"; status: number }
  | { kind: "protocol" };

export type Incoming = { frame: Bytes } | { end: LinkEnd } | { timeout: true };

/** Frames in, in arrival order, then the end. One reader at a time. */
export class Inbox {
  private items: Array<{ frame: Bytes } | { end: LinkEnd }> = [];
  private waiter: (() => void) | null = null;
  private ended: LinkEnd | null = null;

  push(frame: Bytes): void {
    if (this.ended) return;
    this.items.push({ frame });
    this.wake();
  }

  end(end: LinkEnd): void {
    if (this.ended) return;
    this.ended = end;
    this.items.push({ end });
    this.wake();
  }

  private wake(): void {
    const w = this.waiter;
    this.waiter = null;
    w?.();
  }

  async next(timeoutMs?: number): Promise<Incoming> {
    for (;;) {
      const item = this.items[0];
      if (item) {
        if (!("end" in item)) this.items.shift();
        return item;
      }
      let timer: ReturnType<typeof setTimeout> | undefined;
      const woke = await new Promise<boolean>((resolve) => {
        this.waiter = () => resolve(true);
        if (timeoutMs !== undefined) {
          timer = setTimeout(
            () => {
              this.waiter = null;
              resolve(false);
            },
            Math.max(0, timeoutMs),
          );
        }
      });
      if (timer) clearTimeout(timer);
      if (!woke) return { timeout: true };
    }
  }
}

export interface Link {
  readonly kind: "websocket" | "poll";
  /** Resolves once the frame has left; rejects if the link ended first. */
  send(frame: Bytes): Promise<void>;
  /** Long-poll only: the session or pairing id to ask the computer's frames for. */
  setSid(sid: Uint8Array): void;
  close(): void;
}

/** How long to wait before polling again after a busy answer. */
const POLL_BUSY_WAIT_MS = 1000;
/** How many busy answers to one POST before the link gives up. */
const POST_BUSY_TRIES = 5;
const BUFFER_HIGH = P.MAX_FRAME * 2;

class SocketLink implements Link {
  readonly kind = "websocket" as const;
  private closed = false;
  constructor(
    private readonly ws: WebSocketLike,
    private readonly inbox: Inbox,
  ) {
    ws.onmessage = (ev) => {
      const data = ev.data;
      if (!(data instanceof ArrayBuffer)) return;
      if (data.byteLength > P.MAX_FRAME) {
        this.close();
        this.inbox.end({ kind: "protocol" });
        return;
      }
      this.inbox.push(copy(new Uint8Array(data)));
    };
    ws.onclose = (ev) => {
      this.closed = true;
      const code = (ev as { code?: number } | null)?.code ?? 1000;
      this.inbox.end({ kind: "closed", opened: true, code });
    };
    ws.onerror = () => undefined;
  }

  async send(frame: Bytes): Promise<void> {
    while (this.ws.bufferedAmount > BUFFER_HIGH) {
      if (this.closed) break;
      await new Promise((r) => setTimeout(r, 10));
    }
    if (this.closed) throw new Error("link closed");
    this.ws.send(frame);
  }

  setSid(): void {}

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.ws.close();
  }
}

function frameRun(frames: Bytes[]): Bytes {
  return concat(...frames.flatMap((f) => [u32(f.length), f]));
}

function parseRun(body: Uint8Array): Bytes[] | null {
  const frames: Bytes[] = [];
  let at = 0;
  while (at < body.length) {
    if (at + 4 > body.length) return null;
    const n = readU32(body, at);
    at += 4;
    if (n > P.MAX_FRAME || at + n > body.length) return null;
    frames.push(copy(body.subarray(at, at + n)));
    at += n;
  }
  return frames;
}

class PollLink implements Link {
  readonly kind = "poll" as const;
  private queue: Array<{ frame: Bytes; ok: () => void; fail: (e: Error) => void }> = [];
  private posting = false;
  private polling = false;
  private sid: string | null = null;
  private closed = false;
  private readonly abort = new AbortController();

  constructor(
    private readonly url: string,
    private readonly env: Env,
    private readonly inbox: Inbox,
  ) {}

  send(frame: Bytes): Promise<void> {
    return new Promise((ok, fail) => {
      if (this.closed) return fail(new Error("link closed"));
      this.queue.push({ frame, ok, fail });
      void this.pump();
    });
  }

  setSid(sid: Uint8Array): void {
    this.sid = toHex(sid);
    void this.down();
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.abort.abort();
    for (const q of this.queue.splice(0)) q.fail(new Error("link closed"));
  }

  private end(end: LinkEnd): void {
    if (this.closed) return;
    this.close();
    this.inbox.end(end);
  }

  /** A busy answer (409) comes back as a response; every other failure ends the link. */
  private async exchange(init: RequestInit, url: string): Promise<Response | null> {
    try {
      const response = await this.env.fetch(url, {
        ...init,
        cache: "no-store",
        credentials: "omit",
        referrerPolicy: "no-referrer",
        signal: this.abort.signal,
      });
      if (this.closed) return null;
      if (!response.ok && response.status !== HTTP_POLL_BUSY) {
        this.end({ kind: "http", status: response.status });
        return null;
      }
      return response;
    } catch {
      this.end({ kind: "network" });
      return null;
    }
  }

  private async take(response: Response): Promise<boolean> {
    if (response.status === 204) return true;
    if (response.status === HTTP_POLL_BUSY) {
      // Another poll for this id is still held: wait, then poll again.
      await new Promise((r) => setTimeout(r, POLL_BUSY_WAIT_MS));
      return true;
    }
    const frames = parseRun(new Uint8Array(await response.arrayBuffer()));
    if (!frames) {
      this.end({ kind: "protocol" });
      return false;
    }
    for (const f of frames) this.inbox.push(f);
    return true;
  }

  private async pump(): Promise<void> {
    if (this.posting) return;
    this.posting = true;
    let busy = 0;
    while (this.queue.length > 0 && !this.closed) {
      const batch = this.queue.splice(0, P.MAX_LONG_POLL_BATCH);
      const response = await this.exchange(
        {
          method: "POST",
          body: frameRun(batch.map((b) => b.frame)),
          headers: { "content-type": "text/plain" },
        },
        this.url,
      );
      if (response?.status === HTTP_POLL_BUSY) {
        // The computer had no room to read this body: nothing was taken, so send it again.
        busy += 1;
        if (busy > POST_BUSY_TRIES) {
          for (const b of batch) b.fail(new Error("link busy"));
          this.end({ kind: "http", status: HTTP_POLL_BUSY });
          break;
        }
        this.queue.unshift(...batch);
        await new Promise((r) => setTimeout(r, POLL_BUSY_WAIT_MS));
        continue;
      }
      busy = 0;
      if (!response || !(await this.take(response))) {
        for (const b of batch) b.fail(new Error("link ended"));
        break;
      }
      for (const b of batch) b.ok();
    }
    this.posting = false;
  }

  private async down(): Promise<void> {
    if (this.polling || this.closed || !this.sid) return;
    this.polling = true;
    while (!this.closed && this.sid) {
      const response = await this.exchange({ method: "GET" }, `${this.url}?sid=${this.sid}`);
      if (!response || !(await this.take(response))) break;
    }
    this.polling = false;
  }
}

function openSocket(url: string, env: Env, inbox: Inbox, timeoutMs: number): Promise<Link> {
  return new Promise((resolve, reject) => {
    const Ctor = env.WebSocket;
    if (!Ctor) return reject(new Error("no WebSocket"));
    let ws: WebSocketLike;
    try {
      ws = new Ctor(url);
    } catch (e) {
      return reject(e);
    }
    ws.binaryType = "arraybuffer";
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      ws.close();
      reject(new Error("socket timed out"));
    }, timeoutMs);
    ws.onopen = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(new SocketLink(ws, inbox));
    };
    const failed = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(new Error("socket failed"));
    };
    ws.onerror = failed;
    ws.onclose = failed;
  });
}

/** A WebSocket when one opens quickly, otherwise long-poll. */
export async function openLink(
  address: string,
  env: Env,
  inbox: Inbox,
  options: { preferPoll?: boolean; socketTimeoutMs?: number } = {},
): Promise<Link> {
  const origin = originOf(address);
  const url = `${origin}${P.PATH_SESSION}`;
  if (!options.preferPoll && env.WebSocket) {
    try {
      return await openSocket(
        url.replace(/^http/, "ws"),
        env,
        inbox,
        options.socketTimeoutMs ?? 5000,
      );
    } catch {
      // Falls through to long-poll; the caller remembers that the socket did not open.
    }
  }
  return new PollLink(url, env, inbox);
}

// Sessions ----------------------------------------------------------------------------------------

export type OpenFailure = "revoked" | "refused" | "network" | "ended" | "off" | "protocol";

export class OpenError extends Error {
  readonly failure: OpenFailure;
  readonly status: number | undefined;
  /** An opened WebSocket closed before the session was established. */
  readonly socketLost: boolean;
  constructor(failure: OpenFailure, status?: number, socketLost = false) {
    super(failure);
    this.name = "OpenError";
    this.failure = failure;
    this.status = status;
    this.socketLost = socketLost;
  }
}

/** The session ended while a call was waiting on it. The call can be tried on a new session. */
export class SessionClosed extends Error {
  constructor(readonly why: string) {
    super(why);
    this.name = "SessionClosed";
  }
}

/**
 * What an ended link means for an open. Off (503) and the unsealed refusal (403, or a close with
 * 4403) are told apart; a session that ended (410) is tried again; everything else, from no
 * network to a 404, 502 or 530 from the edge, is one state: the computer is asleep or has a
 * new address. The phone never reads those as "turned off".
 */
function failureFor(end: LinkEnd): OpenError {
  switch (end.kind) {
    case "http":
      if (end.status === HTTP_OFF) return new OpenError("off", end.status);
      if (end.status === HTTP_ENDED) return new OpenError("ended", end.status);
      if (end.status === HTTP_REFUSED) return new OpenError("refused", end.status);
      return new OpenError("network", end.status);
    case "network":
      return new OpenError("network");
    case "protocol":
      return new OpenError("protocol");
    case "closed":
      if (end.code === WS_REFUSED) return new OpenError("refused");
      return new OpenError("network", undefined, end.opened);
  }
}

export interface OpenInit {
  address: string;
  device: Pick<PairedDevice, "deviceId" | "privateKey" | "publicKey" | "cpub">;
  env: Env;
  preferPoll?: boolean;
  socketTimeoutMs?: number;
  /** The wait for the computer's answers while opening; the protocol's own limit by default. */
  handshakeMs?: number;
}

export type Answered<T> = { done: T } | undefined;

interface Pending {
  onAnswer: (answer: RemoteAnswer) => Answered<unknown>;
  onChunk?: (chunk: Chunk) => Answered<unknown>;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  nextSeq: number;
}

export class Session {
  readonly sid: Bytes;
  readonly linkKind: "websocket" | "poll";
  readonly openedAt: number;
  /** Cleared when the session is replaced; set by the computer when it removes the phone. */
  revoked = false;
  onRevoked: (() => void) | null = null;
  onEnd: ((why: string) => void) | null = null;

  private upCtr = 0;
  private downCtr = 1;
  private nextId = 1;
  private closedWhy: string | null = null;
  private retired = false;
  private readonly pending = new Map<number, Pending>();
  private tail: Promise<unknown> = Promise.resolve();

  private constructor(
    private readonly link: Link,
    private readonly inbox: Inbox,
    private readonly keys: SessionKeys,
    sid: Bytes,
    readonly hello: Extract<RemoteAnswer, { type: "hello" }>["hello"],
    env: Env,
  ) {
    this.sid = sid;
    this.linkKind = link.kind;
    this.openedAt = env.now();
    void this.pump();
  }

  get expiresAt(): number {
    return this.openedAt + P.SESSION_TTL_SECS * 1000;
  }

  get isOpen(): boolean {
    return this.closedWhy === null;
  }

  get inFlight(): number {
    return this.pending.size;
  }

  /**
   * Opens a session. A WebSocket that opens and then closes before the session is established
   * (a busy computer can drop it) is tried again once, over long-poll.
   */
  static async open(init: OpenInit): Promise<Session> {
    try {
      return await Session.openOnce(init);
    } catch (e) {
      if (e instanceof OpenError && e.socketLost && !init.preferPoll) {
        return Session.openOnce({ ...init, preferPoll: true });
      }
      throw e;
    }
  }

  /** The handshake: SESSION_OPEN, then SESSION_ACCEPT (or a sealed REVOKED), then Hello. */
  private static async openOnce(init: OpenInit): Promise<Session> {
    const { device, env } = init;
    const inbox = new Inbox();
    const link = await openLink(init.address, env, inbox, {
      preferPoll: init.preferPoll,
      socketTimeoutMs: init.socketTimeoutMs,
    });
    const fail = (e: OpenError): never => {
      link.close();
      throw e;
    };
    try {
      const d = device.privateKey;
      if (!d) return fail(new OpenError("refused"));
      const e = await generateSessionKey();
      const on = randomBytes(P.OPEN_NONCE_LEN);
      const kd = await deviceKey(await ecdh(d, device.cpub), device.deviceId);
      const open = await sealSessionOpen(device.deviceId, e.publicKey, on, kd);
      try {
        await link.send(open);
      } catch {
        const got = await inbox.next(0);
        return fail("end" in got ? failureFor(got.end) : new OpenError("network"));
      }

      const started = env.now();
      const budget = init.handshakeMs ?? P.HANDSHAKE_TIMEOUT_SECS * 1000;
      const left = () => budget - (env.now() - started);
      let accepted: ReturnType<typeof openSessionAccept> | null = null;
      while (!accepted) {
        const got = await inbox.next(left());
        if ("timeout" in got) return fail(new OpenError("network"));
        if ("end" in got) return fail(failureFor(got.end));
        const kind = got.frame[1];
        if (kind === P.KIND_SESSION_ACCEPT) {
          try {
            accepted = openSessionAccept(got.frame);
          } catch {
            return fail(new OpenError("refused"));
          }
        } else if (kind === P.KIND_REVOKED) {
          const sealed = await openRevoked(got.frame, kd, device.deviceId, on).then(
            () => true,
            () => false,
          );
          if (sealed) return fail(new OpenError("revoked"));
          // Not an answer to this open: ignored.
        } else {
          return fail(new OpenError("refused"));
        }
      }

      link.setSid(accepted.sid);
      const keys = await phoneSessionKeys(e.privateKey, d, {
        deviceId: device.deviceId,
        epub: e.publicKey,
        openNonce: on,
        sid: accepted.sid,
        fpub: accepted.fpub,
        cpub: device.cpub,
        dpub: device.publicKey,
      });

      const first = await inbox.next(left());
      if ("timeout" in first) return fail(new OpenError("network"));
      if ("end" in first) return fail(failureFor(first.end));
      let reply: RemoteReply;
      try {
        const env1 = await openDataFrame(first.frame, keys.down, "down", accepted.sid, 0);
        if (env1.type !== P.TYPE_JSON) return fail(new OpenError("protocol"));
        reply = JSON.parse(fromUtf8(env1.body)) as RemoteReply;
      } catch (err) {
        return fail(err instanceof OpenError ? err : new OpenError("protocol"));
      }
      if (reply.id !== 0 || reply.answer?.type !== "hello") return fail(new OpenError("protocol"));
      return new Session(link, inbox, keys, accepted.sid, reply.answer.hello, env);
    } catch (err) {
      link.close();
      if (err instanceof OpenError) throw err;
      if (err instanceof Refusal) throw new OpenError("protocol");
      throw new OpenError("network");
    }
  }

  /**
   * Sends one call. `onAnswer` sees each answer to it in order and returns `{ done }` to finish.
   * The call's id is also the id of any chunks it sends or receives.
   */
  request<T>(
    call: RemoteCall,
    onAnswer: (answer: RemoteAnswer) => Answered<T>,
    options: { onChunk?: (chunk: Chunk) => Answered<T> } = {},
  ): { id: number; result: Promise<T> } {
    const id = this.nextId;
    this.nextId += 1;
    const result = new Promise<T>((resolve, reject) => {
      if (this.closedWhy !== null) return reject(new SessionClosed(this.closedWhy));
      this.pending.set(id, {
        onAnswer,
        onChunk: options.onChunk,
        resolve: resolve as (v: unknown) => void,
        reject,
        nextSeq: 0,
      });
      const body = utf8(JSON.stringify({ id, call }));
      this.sendEnvelope(envelopeJson(body)).catch((e: Error) => this.settle(id, e));
    });
    return { id, result };
  }

  /** One chunk of an upload, as a DATA frame. */
  sendChunk(callId: number, seq: number, last: boolean, data: Uint8Array): Promise<void> {
    return this.sendEnvelope(envelopeChunk(callId, seq, last, data));
  }

  private sendEnvelope(envelope: Bytes): Promise<void> {
    const run = this.tail.then(async () => {
      if (this.closedWhy !== null) throw new SessionClosed(this.closedWhy);
      const ctr = this.upCtr + 1;
      const frame = await sealDataFrame(this.keys.up, "up", this.sid, ctr, envelope);
      this.upCtr = ctr;
      try {
        await this.link.send(frame);
      } catch {
        this.fail("send failed");
        throw new SessionClosed("send failed");
      }
    });
    this.tail = run.catch(() => undefined);
    return run;
  }

  private settle(id: number, error: Error): void {
    const p = this.pending.get(id);
    if (!p) return;
    this.pending.delete(id);
    p.reject(error);
    this.maybeFinishRetire();
  }

  /** No new calls are expected on this session; it closes once the ones in flight finish. */
  retire(): void {
    this.retired = true;
    this.maybeFinishRetire();
  }

  private maybeFinishRetire(): void {
    if (this.retired && this.pending.size === 0) this.close("retired");
  }

  close(why = "closed"): void {
    if (this.closedWhy !== null) return;
    this.closedWhy = why;
    this.link.close();
    const dead = new SessionClosed(why);
    for (const [id, p] of this.pending) {
      this.pending.delete(id);
      p.reject(dead);
    }
    this.onEnd?.(why);
  }

  private fail(why: string): void {
    this.close(why);
  }

  private async pump(): Promise<void> {
    for (;;) {
      const got = await this.inbox.next();
      if ("timeout" in got) continue;
      if ("end" in got) return this.fail(`link ${got.end.kind}`);
      try {
        const opened = await openDataFrame(
          got.frame,
          this.keys.down,
          "down",
          this.sid,
          this.downCtr,
        );
        this.downCtr += 1;
        if (opened.type === P.TYPE_CHUNK) this.chunk(chunkParse(opened.body));
        else this.reply(JSON.parse(fromUtf8(opened.body)) as RemoteReply);
      } catch (e) {
        return this.fail(e instanceof Refusal ? e.code : "bad frame");
      }
      if (this.closedWhy !== null) return;
    }
  }

  private chunk(chunk: Chunk): void {
    const p = this.pending.get(chunk.callId);
    if (!p) return;
    if (chunk.seq !== p.nextSeq) {
      this.fail("chunkGap");
      return;
    }
    p.nextSeq += 1;
    const out = p.onChunk?.(chunk);
    if (out) {
      this.pending.delete(chunk.callId);
      p.resolve(out.done);
      this.maybeFinishRetire();
    }
  }

  private reply(reply: RemoteReply): void {
    const answer = reply?.answer;
    if (typeof reply?.id !== "number" || !answer || typeof answer.type !== "string") {
      this.fail("bad reply");
      return;
    }
    if (answer.type === "revoked") {
      this.revoked = true;
      this.onRevoked?.();
      this.close("revoked");
      return;
    }
    const p = this.pending.get(reply.id);
    if (!p) return;
    if (answer.type === "error") {
      this.settle(reply.id, new PhoneRequestError(answer.code));
      return;
    }
    const out = p.onAnswer(answer);
    if (out) {
      this.pending.delete(reply.id);
      p.resolve(out.done);
      this.maybeFinishRetire();
    }
  }
}
