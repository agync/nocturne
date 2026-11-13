// Pairing, from the phone: the same protocol whether the code was scanned or typed. The phone
// commits once per page load and never retries; every frame it sends carries a fresh random
// nonce, so a reload cannot repeat one.

import { type Bytes, fromBase64Url, fromUtf8, toBase64Url, utf8 } from "./bytes";
import { type Env, Inbox, type Link, openLink } from "./channel";
import {
  checkDeviceName,
  checkWords,
  codeDecode,
  codeNormalise,
  commitment,
  envelopeJson,
  generateDeviceKey,
  helloRequest,
  openHelloAnswer,
  openPairFrame,
  pairingId,
  pairKeys,
  parsePairingFragment,
  randomBytes,
  sealPairFrame,
} from "./crypto";
import type { KeyStore, PairedDevice } from "./engine";
import * as P from "./protocol";

export type PairingResult =
  | { kind: "allowed"; device: PairedDevice }
  | { kind: "refused" }
  | { kind: "expired" }
  | { kind: "spent" };

export interface PairingContext {
  env: Env;
  /** The page's own host: the address the words are bound to, and where frames go. */
  host: string;
  /** The page's origin: the phone's home. */
  origin: string;
  store: KeyStore;
  preferPoll?: boolean;
  socketTimeoutMs?: number;
}

/** A typed code or a link that cannot start a pairing. */
export class PairingError extends Error {
  constructor(readonly kind: "badCode" | "expired" | "spent") {
    super(kind);
    this.name = "PairingError";
  }
}

/** The pairing link's fragment, or null; the page removes the fragment from its address after. */
export function offerFromFragment(fragment: string): { s: Bytes; cpub: Bytes } | null {
  return parsePairingFragment(fragment);
}

export class Pairing {
  private committed = false;

  private constructor(
    private readonly ctx: PairingContext,
    private readonly path: number,
    private readonly pid: Bytes,
    private readonly cpub: Bytes,
    private readonly keys: { up: Bytes; down: Bytes },
  ) {}

  /** A scanned link: its fragment holds S and the computer's key. */
  static async fromLink(offer: { s: Bytes; cpub: Bytes }, ctx: PairingContext): Promise<Pairing> {
    const pid = await pairingId(offer.s);
    const { up, down } = await pairKeys(offer.s, offer.cpub, pid);
    return new Pairing(ctx, P.PATH_SCANNED, pid, offer.cpub, { up, down });
  }

  /** A typed code: the computer's key comes from an unsealed HELLO, answered while the offer is open. */
  static async fromCode(typed: string, ctx: PairingContext): Promise<Pairing> {
    let s: Bytes;
    try {
      s = codeDecode(codeNormalise(typed));
    } catch {
      throw new PairingError("badCode");
    }
    const pid = await pairingId(s);
    const inbox = new Inbox();
    const link = await openLink(ctx.host, ctx.env, inbox, {
      preferPoll: ctx.preferPoll,
      socketTimeoutMs: ctx.socketTimeoutMs,
    });
    try {
      await link.send(helloRequest(pid));
      const got = await inbox.next(P.HANDSHAKE_TIMEOUT_SECS * 1000);
      if (!("frame" in got)) throw new PairingError("expired");
      const cpub = openHelloAnswer(got.frame, pid);
      const { up, down } = await pairKeys(s, cpub, pid);
      return new Pairing(ctx, P.PATH_TYPED, pid, cpub, { up, down });
    } catch (e) {
      if (e instanceof PairingError) throw e;
      throw new PairingError("expired");
    } finally {
      link.close();
    }
  }

  get spentAlready(): boolean {
    return this.committed;
  }

  /**
   * Commit, reveal, show the words, then wait for the person's decision on the computer. Calling
   * this a second time on the same page load gives `spent` without sending anything.
   */
  async connect(deviceName: string, onWords: (words: string[]) => void): Promise<PairingResult> {
    if (this.committed) return { kind: "spent" };
    const name = deviceName.trim();
    const nameRaw = checkDeviceName(name);
    this.committed = true;

    const device = await generateDeviceKey();
    const np = randomBytes(P.NONCE_PAIR_LEN);
    const commit = await commitment(device.publicKey, np, nameRaw);
    const inbox = new Inbox();
    let link: Link;
    try {
      link = await openLink(this.ctx.host, this.ctx.env, inbox, {
        preferPoll: this.ctx.preferPoll,
        socketTimeoutMs: this.ctx.socketTimeoutMs,
      });
    } catch {
      return { kind: "expired" };
    }
    try {
      const send = async (kind: number, body: object) =>
        link.send(
          await sealPairFrame(
            kind,
            this.pid,
            this.keys.up,
            randomBytes(P.GCM_NONCE_LEN),
            envelopeJson(utf8(JSON.stringify(body))),
          ),
        );
      const open = (frame: Bytes, kind: number) =>
        openPairFrame(frame, kind, this.pid, this.keys.down);

      link.setSid(this.pid);
      await send(P.KIND_PAIR_COMMIT, {
        path: this.path,
        deviceName: name,
        commit: toBase64Url(commit),
      });

      let nc: Bytes | null = null;
      let deadline = this.ctx.env.now() + P.HANDSHAKE_TIMEOUT_SECS * 1000;
      for (;;) {
        const got = await inbox.next(Math.max(deadline - this.ctx.env.now(), 0));
        if ("timeout" in got || "end" in got) return { kind: nc ? "spent" : "expired" };
        const kind = got.frame[1];
        if (kind === P.KIND_PAIR_NONCE) {
          if (nc) continue;
          const body = JSON.parse(fromUtf8((await open(got.frame, P.KIND_PAIR_NONCE)).body)) as {
            nc?: string;
          };
          const received = fromBase64Url(String(body.nc));
          if (received.length !== P.NONCE_PAIR_LEN) return { kind: "spent" };
          nc = received;
          deadline = this.ctx.env.now() + (P.DECIDE_TTL_SECS + 20) * 1000;
          await send(P.KIND_PAIR_REVEAL, {
            dpub: toBase64Url(device.publicKey),
            np: toBase64Url(np),
          });
          const words = await checkWords(device.privateKey, {
            path: this.path,
            pairingId: this.pid,
            host: this.ctx.host,
            cpub: this.cpub,
            dpub: device.publicKey,
            nameRaw,
            np,
            nc,
          });
          onWords(words);
        } else if (kind === P.KIND_PAIR_OUTCOME) {
          if (!nc) return { kind: "spent" };
          const body = JSON.parse(fromUtf8((await open(got.frame, P.KIND_PAIR_OUTCOME)).body)) as {
            allowed?: boolean;
            deviceId?: string;
          };
          if (body.allowed !== true) return { kind: "refused" };
          const deviceId = fromBase64Url(String(body.deviceId));
          if (deviceId.length !== P.DEVICE_ID_LEN) return { kind: "spent" };
          const paired: PairedDevice = {
            deviceId,
            privateKey: device.privateKey,
            publicKey: device.publicKey,
            cpub: this.cpub,
            homeOrigin: this.ctx.origin,
            address: this.ctx.host,
            deviceName: name,
            computerName: null,
            connection: null,
            version: null,
            pairedAt: this.ctx.env.now(),
            lastConnectedAt: null,
          };
          await this.ctx.store.save(paired);
          return { kind: "allowed", device: paired };
        } else {
          return { kind: "spent" };
        }
      }
    } catch {
      return { kind: "spent" };
    } finally {
      link.close();
    }
  }
}
