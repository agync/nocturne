// The phone's own memory, behind the same `PhoneEngine` the companion screens use. Every call goes
// to the app's Rust side (`home_call`), which answers from the store on this phone. Nothing here
// reaches a network; there is no computer to be away from, so the connection is always open.

import { invoke } from "@tauri-apps/api/core";
import type { PhoneHello, PhoneKept, RemoteAnswer, RemoteCall } from "../../engine/bindings";
import { toBase64Url } from "../bytes";
import { type ConnectedEngine, type Connection, PhoneRequestError } from "../engine";
import { asError, marked, mimeOf, SLICE, unframe } from "../native/engine";
import * as P from "../protocol";

/** What the screens read from a hello. A phone alone has no computer, so the names are its own. */
export function homeHello(deviceName: string, version: string): PhoneHello {
  return {
    computerName: deviceName,
    deviceName,
    connection: "simple",
    version,
    minPhoneBuild: P.PHONE_BUILD,
    maxPhoneBuild: P.PHONE_BUILD,
    features: ["calendar", "health"],
  };
}

async function run<T>(cmd: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (typeof e === "string") throw new PhoneRequestError(e as PhoneRequestError["code"]);
    throw asError(e);
  }
}

function expect<K extends RemoteAnswer["type"]>(
  answer: RemoteAnswer,
  kind: K,
): Extract<RemoteAnswer, { type: K }> {
  if (answer.type === "error") throw new PhoneRequestError(answer.code);
  if (answer.type !== kind) throw new PhoneRequestError("invalid");
  return answer as Extract<RemoteAnswer, { type: K }>;
}

const once = async <K extends RemoteAnswer["type"]>(call: RemoteCall, kind: K) =>
  expect(await run<RemoteAnswer>("home_call", { call }), kind);

export function createLocalEngine(options: {
  deviceName(): string;
  version: string;
}): ConnectedEngine {
  const connected = (): Connection => ({
    kind: "connected",
    hello: homeHello(options.deviceName(), options.version),
    agreed: P.PHONE_BUILD,
  });
  let state = connected();
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
      return expect(await run<RemoteAnswer>("home_keep_end", { handle }), "kept").kept;
    };
    const next = exclusive.then(go, go);
    exclusive = next.catch(() => undefined);
    return next;
  }

  return {
    connection: () => state,
    subscribe: () => () => undefined,
    async connect() {
      state = connected();
      return state;
    },
    close() {},
    async hello() {
      return homeHello(options.deviceName(), options.version);
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
    async getLookBack() {
      throw new PhoneRequestError("notReady");
    },
    async lookBack() {
      throw new PhoneRequestError("notReady");
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
    async ask() {
      throw new PhoneRequestError("notReady");
    },
    async cancelAsk() {},
    async seeOnlineChange() {},
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
      return unframe(await run<ArrayBuffer | number[]>("home_original", { id }));
    },
    async disconnect() {
      throw new PhoneRequestError("invalid");
    },
  };
}
