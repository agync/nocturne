// The phone app's engine: every call goes to the app's Rust side through `invoke`, which holds
// the keys and the sealed channel. The web view itself reaches nothing on the network.

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AskDelta,
  PhoneError,
  PhoneKept,
  RemoteAnswer,
  RemoteCall,
} from "../../engine/bindings";
import { toBase64Url } from "../bytes";
import { checkMagic, Refusal, sniffMime } from "../crypto";
import {
  type ConnectedEngine,
  type Connection,
  ConnectionError,
  type KeepMarks,
  PhoneRequestError,
} from "../engine";
import * as P from "../protocol";

/**
 * The bytes of a keep go to the Rust side in slices of this size, as base64url text: Android's
 * web view carries no raw IPC body, and a whole photo as one message would not fit.
 */
export const SLICE = 384 * 1024;

/** How a call failed on the Rust side. */
export type CallFailure =
  | { type: "phone"; code: PhoneError }
  | { type: "connection"; state: Exclude<Connection["kind"], "connecting" | "connected"> };

/** The Rust side's failure as the error the screens already handle. */
export function asError(e: unknown): Error {
  const f = e as Partial<CallFailure> | null;
  if (f && f.type === "phone" && typeof f.code === "string") return new PhoneRequestError(f.code);
  if (f && f.type === "connection" && typeof f.state === "string")
    return new ConnectionError(f.state);
  return e instanceof Error ? e : new ConnectionError("asleep");
}

async function run<T>(cmd: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw asError(e);
  }
}

function expect<K extends RemoteAnswer["type"]>(
  answer: RemoteAnswer,
  kind: K,
): Extract<RemoteAnswer, { type: K }> {
  if (answer.type !== kind) throw new PhoneRequestError("invalid");
  return answer as Extract<RemoteAnswer, { type: K }>;
}

const once = async <K extends RemoteAnswer["type"]>(call: RemoteCall, kind: K) =>
  expect(await run<RemoteAnswer>("call", { call }), kind);

/** The allowed type for a blob, from its own type or, failing that, its first bytes. */
async function mimeOf(blob: Blob): Promise<string> {
  const head = new Uint8Array(await blob.slice(0, 32).arrayBuffer());
  const declared = (blob.type.split(";")[0] ?? "").trim().toLowerCase();
  const mime = P.MIME_MAGIC.some((m) => m.mime === declared) ? declared : sniffMime(head);
  if (!mime) throw new PhoneRequestError("invalid");
  try {
    checkMagic(mime, head);
  } catch (e) {
    if (e instanceof Refusal) throw new PhoneRequestError("invalid");
    throw e;
  }
  return mime;
}

function marked(marks: KeepMarks | undefined) {
  const out: {
    feeling?: KeepMarks["feeling"];
    good?: boolean;
    capturedAt?: string;
    web?: boolean;
  } = {};
  if (marks?.feeling) out.feeling = marks.feeling;
  if (marks?.good !== undefined) out.good = marks.good;
  if (marks?.capturedAt) out.capturedAt = marks.capturedAt;
  if (marks?.web) out.web = true;
  return out;
}

/**
 * `[type length][type][bytes]`, as the Rust side frames an original. It comes as an
 * `ArrayBuffer`, or as an array of numbers where the web view's IPC carries no raw body.
 */
export function unframe(data: ArrayBuffer | ArrayLike<number>): Blob {
  const all = data instanceof ArrayBuffer ? new Uint8Array(data) : Uint8Array.from(data);
  const n = all[0] ?? 0;
  const mime = new TextDecoder().decode(all.subarray(1, 1 + n));
  const allowed = P.MIME_MAGIC.find((m) => m.mime === mime);
  if (!allowed) throw new PhoneRequestError("invalid");
  return new Blob([all.subarray(1 + n)], { type: allowed.mime });
}

export function createNativeEngine(options: { onForgotten?: () => void } = {}): ConnectedEngine {
  let state: Connection = { kind: "connecting" };
  const listeners = new Set<(c: Connection) => void>();
  const set = (next: Connection) => {
    state = next;
    if (next.kind === "removed") options.onForgotten?.();
    for (const l of [...listeners]) l(next);
  };
  const unlisten = listen<Connection>("connection", (e) => set(e.payload));
  let exclusive: Promise<unknown> = Promise.resolve();

  async function keepWith(call: RemoteCall, blob: Blob): Promise<PhoneKept> {
    const go = async () => {
      const handle = await run<number>("keep_begin", { call });
      try {
        for (let at = 0; at < blob.size; at += SLICE) {
          const part = new Uint8Array(await blob.slice(at, at + SLICE).arrayBuffer());
          await run("keep_part", { handle, slice: toBase64Url(part) });
        }
      } catch (e) {
        void invoke("keep_cancel", { handle }).catch(() => undefined);
        throw e;
      }
      return expect(await run<RemoteAnswer>("keep_end", { handle }), "kept").kept;
    };
    const next = exclusive.then(go, go);
    exclusive = next.catch(() => undefined);
    return next;
  }
  return {
    connection: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async connect() {
      const next = await invoke<Connection>("connect").catch(() => ({ kind: "asleep" }) as const);
      set(next);
      return next;
    },
    close() {
      void unlisten.then((u) => u());
    },
    async hello() {
      const c = await this.connect();
      if (c.kind !== "connected") throw new ConnectionError(c.kind);
      return c.hello;
    },
    async status() {
      return (await once({ cmd: "getStatus" }, "status")).status;
    },
    async keepText(text, clientId, marks) {
      if (text.trim() === "") throw new PhoneRequestError("invalid");
      if (new TextEncoder().encode(text).length > P.MAX_TEXT)
        throw new PhoneRequestError("tooLarge");
      return (await once({ cmd: "keepText", text, clientId, ...marked(marks) }, "kept")).kept;
    },
    async keepBytes(blob, clientId, caption, marks) {
      const note = (caption ?? "").trim();
      if (blob.size < 1) throw new PhoneRequestError("invalid");
      if (blob.size > P.MAX_ITEM) throw new PhoneRequestError("tooLarge");
      const mime = await mimeOf(blob);
      if (marks?.good && !mime.startsWith("image/")) throw new PhoneRequestError("invalid");
      return keepWith(
        {
          cmd: "keepBytes",
          mime,
          size: blob.size,
          clientId,
          caption: note === "" ? null : note,
          ...marked(marks),
        },
        blob,
      );
    },
    async keepRecording(blob, clientId, marks) {
      if (marks?.good) throw new PhoneRequestError("invalid");
      if (blob.size < 1) throw new PhoneRequestError("invalid");
      if (blob.size > P.MAX_ITEM) throw new PhoneRequestError("tooLarge");
      const mime = await mimeOf(blob);
      return keepWith(
        { cmd: "keepRecording", mime, size: blob.size, clientId, ...marked(marks) },
        blob,
      );
    },
    async keepCheckIn(input, clientId, capturedAt) {
      if (input.replaces) throw new PhoneRequestError("invalid");
      const call: RemoteCall = {
        cmd: "keepCheckIn",
        input,
        clientId,
        ...(capturedAt ? { capturedAt } : {}),
      };
      return (await once(call, "checkIn")).checkin;
    },
    async getJournalDay(date) {
      return (await once({ cmd: "getJournalDay", date: date ?? null }, "journalDay")).day;
    },
    async getLookBack(period) {
      return (await once({ cmd: "getLookBack", period }, "lookBack")).view;
    },
    async lookBack(period) {
      return (await once({ cmd: "lookBack", period }, "lookBack")).view;
    },
    async answerSameName(questionId, answer) {
      return (await once({ cmd: "answerSameName", questionId, answer }, "sameName")).answered;
    },
    async answerJournalQuestion(pageDate, choice, clientId, line) {
      const said = (line ?? "").trim();
      const call: RemoteCall = {
        cmd: "answerJournalQuestion",
        pageDate,
        choice,
        clientId,
        ...(said ? { line: said } : {}),
      };
      return (await once(call, "page")).page;
    },
    async unkeep(id) {
      await once({ cmd: "unkeep", id }, "done");
    },
    async ask(question, onDelta, place) {
      const deltas = new Channel<AskDelta>();
      deltas.onmessage = onDelta;
      return run<string>("ask", { question, place: place ?? null, onDelta: deltas });
    },
    async seeOnlineChange() {
      await once({ cmd: "seeOnlineChange" }, "done");
    },
    async cancelAsk(askId) {
      await once({ cmd: "cancelAsk", askId }, "done");
    },
    async getPage(date) {
      return (await once({ cmd: "getPage", date: date ?? null }, "page")).page;
    },
    async listPages() {
      return (await once({ cmd: "listPages" }, "pages")).pages;
    },
    async getItem(id) {
      return (await once({ cmd: "getItem", id }, "item")).item;
    },
    async original(id) {
      return unframe(await run<ArrayBuffer | number[]>("original", { id }));
    },
    async disconnect() {
      const out = await run<{ reached: boolean }>("disconnect");
      set({ kind: "cantConnect" });
      options.onForgotten?.();
      return out;
    },
  };
}
