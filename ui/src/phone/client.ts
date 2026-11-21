// The phone's engine over the sealed channel: the allowed calls, the journal calls and the two
// phone verbs, a session opened when one is needed and renewed before it lapses, and a
// connection state the screens read. Calls are retried once on a new session after a transport
// error; a keep carries its client id, so the computer keeps it once.

import type {
  AskDelta,
  CheckIn,
  Feeling,
  JournalDay,
  PageSummary,
  PageView,
  Period,
  PhoneAppIntro,
  PhoneHello,
  PhoneItem,
  PhoneKept,
  PhoneStatus,
  ReflectionView,
  RemoteAnswer,
  RemoteCall,
  SameNameAnswer,
  SameNameAnswered,
} from "../engine/bindings";
import { type Bytes, copy, utf8 } from "./bytes";
import {
  defaultEnv,
  type Env,
  OpenError,
  type OpenFailure,
  type Session,
  Session as SessionClass,
  SessionClosed,
} from "./channel";
import { checkId, checkMagic, Refusal, sniffMime } from "./crypto";
import {
  type ConnectedEngine,
  type Connection,
  ConnectionError,
  type KeepMarks,
  type KeyStore,
  PhoneRequestError,
} from "./engine";
import * as P from "./protocol";
import { sendBlob } from "./upload";

export interface EngineOptions {
  env?: Env;
  /** Skip the WebSocket and use long-poll from the start. */
  preferPoll?: boolean;
  socketTimeoutMs?: number;
  handshakeMs?: number;
  /** A sealed revocation arrived: the keys are already forgotten. */
  onRevoked?: () => void;
  /** Open a replacement session this long before the old one lapses. */
  renewMarginMs?: number;
  /** What this page says of itself first on each session, to a computer that listens. */
  app?: PhoneAppIntro;
}

const type =
  <K extends RemoteAnswer["type"]>(kind: K) =>
  (answer: RemoteAnswer): { done: Extract<RemoteAnswer, { type: K }> } | undefined =>
    answer.type === kind ? { done: answer as Extract<RemoteAnswer, { type: K }> } : undefined;

/** The allowed type for a blob, from its own type or, failing that, its first bytes. */
async function mimeOf(blob: Blob): Promise<string> {
  const head = new Uint8Array(await blob.slice(0, 32).arrayBuffer());
  const declared = (blob.type.split(";")[0] ?? "").trim().toLowerCase();
  const sniffed = sniffMime(head);
  const mime = P.MIME_MAGIC.some((m) => m.mime === declared) ? declared : sniffed;
  if (!mime) throw new PhoneRequestError("invalid");
  try {
    checkMagic(mime, head);
  } catch (e) {
    if (e instanceof Refusal) throw new PhoneRequestError("invalid");
    throw e;
  }
  return mime;
}

/** Everything that is not off, refused or removed is one state: asleep, or at a new address. */
function connectionFor(failure: OpenFailure): "removed" | "cantConnect" | "asleep" | "turnedOff" {
  switch (failure) {
    case "revoked":
      return "removed";
    case "refused":
    case "protocol":
      return "cantConnect";
    case "off":
      return "turnedOff";
    case "network":
    case "ended":
      return "asleep";
  }
}

export function createPhoneEngine(
  store: KeyStore,
  address: () => string,
  options: EngineOptions = {},
): ConnectedEngine {
  const env = options.env ?? defaultEnv();
  const margin = options.renewMarginMs ?? 60_000;
  let session: Session | null = null;
  let opening: Promise<Session> | null = null;
  let state: Connection = { kind: "connecting" };
  let stickyPoll = options.preferPoll ?? false;
  let closed = false;
  // Set while disconnect runs: an open that is still racing it must not save the device again.
  let disconnecting = false;
  let upload: Promise<unknown> = Promise.resolve();
  const listeners = new Set<(c: Connection) => void>();

  const set = (next: Connection) => {
    if (state.kind === next.kind && state.kind !== "connected") return;
    state = next;
    for (const l of [...listeners]) l(next);
  };

  const forget = async () => {
    await store.clear();
    set({ kind: "removed" });
    options.onRevoked?.();
  };

  async function openNew(): Promise<Session> {
    if (closed) throw new ConnectionError("asleep");
    const device = await store.device();
    if (closed) throw new ConnectionError("asleep");
    if (!device) {
      set({ kind: "cantConnect" });
      throw new ConnectionError("cantConnect");
    }
    if (state.kind !== "connected") set({ kind: "connecting" });
    const host = address();
    try {
      const init = {
        address: host,
        device,
        env,
        preferPoll: stickyPoll,
        socketTimeoutMs: options.socketTimeoutMs,
        handshakeMs: options.handshakeMs,
      };
      // A session that ended under the open (410) is opened again, once.
      const opened = await SessionClass.open(init).catch((e: unknown) => {
        if (e instanceof OpenError && e.failure === "ended") return SessionClass.open(init);
        throw e;
      });
      const needed = opened.hello.minPhoneBuild;
      if (needed > P.PHONE_BUILD) {
        // This phone's app is older than the computer wants: keys stay, nothing is saved.
        opened.close("outdated");
        set({ kind: "outdated", minBuild: needed, computer: opened.hello.version });
        throw new ConnectionError("outdated");
      }
      if (options.app && (opened.hello.maxPhoneBuild ?? needed) >= P.INTRODUCE_BUILD) {
        const said = await opened
          .request({ cmd: "introduce", app: options.app }, type("introduced"))
          .result.then(
            () => null,
            (e: unknown) => (e instanceof PhoneRequestError ? e.code : null),
          );
        // A page kept from before the computer last updated: the same as an old app.
        if (said === "tooOld") {
          opened.close("outdated");
          set({ kind: "outdated", minBuild: needed, computer: opened.hello.version });
          throw new ConnectionError("outdated");
        }
      }
      if (opened.linkKind === "poll" && env.WebSocket) stickyPoll = true;
      const previous = session;
      session = opened;
      previous?.retire();
      opened.onRevoked = () => void forget();
      opened.onEnd = () => {
        if (session === opened) {
          session = null;
          if (!closed && state.kind === "connected") set({ kind: "connecting" });
        }
      };
      // The address is stored only now: a session to it has opened and Hello has been read. A
      // record that is gone (the person disconnected, or removed the phone) stays gone.
      const latest = await store.device();
      if (closed || !latest) {
        opened.close("forgotten");
        if (session === opened) session = null;
        set({ kind: "cantConnect" });
        throw new ConnectionError(closed ? "asleep" : "cantConnect");
      }
      if (!disconnecting)
        await store.save({
          ...latest,
          address: host,
          deviceName: opened.hello.deviceName,
          computerName: opened.hello.computerName,
          connection: opened.hello.connection,
          version: opened.hello.version,
          lastConnectedAt: env.now(),
        });
      set({ kind: "connected", hello: opened.hello });
      return opened;
    } catch (e) {
      if (e instanceof OpenError) {
        if (e.failure === "revoked") {
          await forget();
          throw new ConnectionError("removed");
        }
        const kind = connectionFor(e.failure);
        set({ kind } as Connection);
        throw new ConnectionError(kind, e.status);
      }
      throw e;
    }
  }

  async function ensure(): Promise<Session> {
    if (closed) throw new ConnectionError("asleep");
    const cur = session;
    if (cur?.isOpen && env.now() < cur.expiresAt - margin) return cur;
    if (!opening) {
      opening = openNew().finally(() => {
        opening = null;
      });
    }
    return opening;
  }

  /** Runs `fn` on a session; after a transport error or an expired session, once more on a new one. */
  async function call<T>(fn: (s: Session) => Promise<T>, retry = true): Promise<T> {
    for (let attempt = 0; ; attempt += 1) {
      const s = await ensure();
      try {
        return await fn(s);
      } catch (e) {
        const dead =
          e instanceof SessionClosed ||
          (e instanceof PhoneRequestError && e.code === "sessionExpired");
        if (e instanceof PhoneRequestError && e.code === "revoked") {
          await forget();
          throw new ConnectionError("removed");
        }
        if (s.revoked) throw new ConnectionError("removed");
        if (!dead) throw e;
        s.close("replaced");
        if (session === s) session = null;
        if (!retry || attempt >= 1) throw new ConnectionError("asleep");
      }
    }
  }

  const once = <A extends RemoteAnswer["type"]>(call_: RemoteCall, kind: A, retry = true) =>
    call((s) => s.request(call_, type(kind)).result, retry);

  const exclusive = <T>(fn: () => Promise<T>): Promise<T> => {
    const run = upload.then(fn, fn);
    upload = run.catch(() => undefined);
    return run;
  };

  async function keep(
    call_: Extract<RemoteCall, { cmd: "keepBytes" | "keepRecording" }>,
    blob: Blob,
  ): Promise<PhoneKept> {
    return exclusive(() =>
      call(async (s) => {
        const { id, result } = s.request(call_, type("kept"));
        let ended = false;
        const settled = result.then(
          (r) => {
            ended = true;
            return r;
          },
          (e) => {
            ended = true;
            throw e;
          },
        );
        const sending = sendBlob(
          { chunk: (seq, last, data) => s.sendChunk(id, seq, last, data) },
          blob,
          () => ended,
        );
        const [got, sent] = await Promise.allSettled([settled, sending]);
        if (got.status === "rejected") throw got.reason;
        if (sent.status === "rejected") throw sent.reason;
        return got.value.kept;
      }),
    );
  }

  const engine: ConnectedEngine = {
    connection: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async connect() {
      try {
        await ensure();
      } catch (e) {
        if (!(e instanceof ConnectionError || e instanceof OpenError)) throw e;
      }
      return state;
    },
    close() {
      closed = true;
      session?.close("tab closed");
      session = null;
    },

    async hello(): Promise<PhoneHello> {
      return (await ensure()).hello;
    },
    async status(): Promise<PhoneStatus> {
      return (await once({ cmd: "getStatus" }, "status")).status;
    },
    async keepText(text, clientId, marks): Promise<PhoneKept> {
      const size = utf8(text).length;
      if (text.trim() === "") throw new PhoneRequestError("invalid");
      if (size > P.MAX_TEXT) throw new PhoneRequestError("tooLarge");
      checkClientId(clientId);
      checkMarks(marks);
      return (await once({ cmd: "keepText", text, clientId, ...sent(marks) }, "kept")).kept;
    },
    async keepBytes(blob, clientId, caption, marks) {
      checkClientId(clientId);
      checkMarks(marks);
      const note = (caption ?? "").trim();
      if ([...note].length > P.MAX_CAPTION) throw new PhoneRequestError("invalid");
      if (blob.size < 1) throw new PhoneRequestError("invalid");
      if (blob.size > P.MAX_ITEM) throw new PhoneRequestError("tooLarge");
      const mime = await mimeOf(blob);
      // Only a photo can be a good thing; a document cannot.
      if (marks?.good && !mime.startsWith("image/")) throw new PhoneRequestError("invalid");
      return keep(
        {
          cmd: "keepBytes",
          mime,
          size: blob.size,
          clientId,
          caption: note === "" ? null : note,
          ...sent(marks),
        },
        blob,
      );
    },
    async keepRecording(blob, clientId, marks) {
      checkClientId(clientId);
      checkMarks(marks);
      if (marks?.good) throw new PhoneRequestError("invalid");
      if (blob.size < 1) throw new PhoneRequestError("invalid");
      if (blob.size > P.MAX_ITEM) throw new PhoneRequestError("tooLarge");
      const mime = await mimeOf(blob);
      return keep({ cmd: "keepRecording", mime, size: blob.size, clientId, ...sent(marks) }, blob);
    },
    async keepCheckIn(input, clientId, capturedAt): Promise<CheckIn> {
      checkClientId(clientId);
      checkFeeling(input.feeling);
      // Changing a check-in is for the computer.
      if (input.replaces) throw new PhoneRequestError("invalid");
      const said = utf8(input.line ?? "").length + utf8(input.urge ?? "").length;
      if (said > P.MAX_TEXT) throw new PhoneRequestError("tooLarge");
      return (
        await once(
          { cmd: "keepCheckIn", input, clientId, ...(capturedAt ? { capturedAt } : {}) },
          "checkIn",
        )
      ).checkin;
    },
    async getJournalDay(date): Promise<JournalDay> {
      if (date !== undefined) checkDate(date);
      return (await once({ cmd: "getJournalDay", date: date ?? null }, "journalDay")).day;
    },
    async getLookBack(period): Promise<ReflectionView> {
      checkPeriod(period);
      return (await once({ cmd: "getLookBack", period }, "lookBack")).view;
    },
    async lookBack(period): Promise<ReflectionView> {
      checkPeriod(period);
      return (await once({ cmd: "lookBack", period }, "lookBack")).view;
    },
    async answerSameName(questionId, answer): Promise<SameNameAnswered> {
      checkClientId(questionId);
      checkAnswer(answer);
      return (await once({ cmd: "answerSameName", questionId, answer }, "sameName")).answered;
    },
    async answerJournalQuestion(pageDate, choice, clientId, line): Promise<PageView> {
      checkClientId(clientId);
      checkDate(pageDate);
      if (choice !== "fine" && choice !== "notGreat" && choice !== "notYet")
        throw new PhoneRequestError("invalid");
      const said = (line ?? "").trim();
      if (utf8(said).length > P.MAX_TEXT) throw new PhoneRequestError("tooLarge");
      const request: RemoteCall = {
        cmd: "answerJournalQuestion",
        pageDate,
        choice,
        clientId,
        ...(said ? { line: said } : {}),
      };
      return (await once(request, "page")).page;
    },
    async unkeep(id) {
      checkItemId(id);
      await once({ cmd: "unkeep", id }, "done");
    },
    async ask(question, onDelta, place): Promise<string> {
      if (utf8(question).length > P.MAX_QUESTION) throw new PhoneRequestError("invalid");
      if (question.trim() === "") throw new PhoneRequestError("invalid");
      return call(async (s) => {
        // An answer that cannot be sent again still finds out why its session ended: a phone that
        // was removed is told by the refusal to its next open, and says so now, not on a later tap.
        // Nothing starts once the phone has disconnected or the tab has closed.
        const learn = (e: unknown) => {
          if (e instanceof SessionClosed && !closed) void ensure().catch(() => undefined);
          return e;
        };
        let askId = "";
        const request: RemoteCall = { cmd: "ask", question, ...(place ? { place } : {}) };
        const { result } = s.request(request, (answer) => {
          if (answer.type === "askStarted") askId = answer.askId;
          if (answer.type === "askDelta") {
            const delta: AskDelta = answer.delta;
            onDelta(delta);
            if (TERMINAL.has(delta.phase)) return { done: delta.askId };
          }
          return undefined;
        });
        const id = await result.catch((e: unknown) => {
          throw learn(e);
        });
        return askId || id;
      }, false);
    },
    async seeOnlineChange() {
      await once({ cmd: "seeOnlineChange" }, "done");
    },
    async cancelAsk(askId) {
      checkClientId(askId);
      await once({ cmd: "cancelAsk", askId }, "done");
    },
    async getPage(date): Promise<PageView> {
      return (await once({ cmd: "getPage", date: date ?? null }, "page")).page;
    },
    async listPages(): Promise<PageSummary[]> {
      return (await once({ cmd: "listPages" }, "pages")).pages;
    },
    async getItem(id): Promise<PhoneItem> {
      checkItemId(id);
      return (await once({ cmd: "getItem", id }, "item")).item;
    },
    async original(id): Promise<Blob> {
      checkItemId(id);
      return call(async (s) => {
        const parts: Bytes[] = [];
        // Set once, by the one originalStart, to a type taken from the allowlist itself.
        let meta: Readonly<{ mime: string; size: number }> | null = null;
        let total = 0;
        const { result } = s.request(
          { cmd: "getOriginal", id },
          (answer) => {
            if (answer.type !== "originalStart") return undefined;
            // One start only, before any byte; a later one cannot change what was checked.
            if (meta !== null || parts.length > 0) throw new Refusal("badEnvelope");
            const allowed = P.MIME_MAGIC.find((x) => x.mime === answer.mime);
            if (!allowed) throw new Refusal("badMagic");
            if (!Number.isInteger(answer.size) || answer.size < 1 || answer.size > P.MAX_ITEM) {
              throw new Refusal("badEnvelope");
            }
            meta = Object.freeze({ mime: allowed.mime, size: answer.size });
            return undefined;
          },
          {
            onChunk: (chunk) => {
              const m = meta as Readonly<{ mime: string; size: number }> | null;
              if (!m) throw new Refusal("badEnvelope");
              if (parts.length === 0) checkMagic(m.mime, chunk.data);
              parts.push(copy(chunk.data));
              total += chunk.data.length;
              if (total > m.size) throw new Refusal("badEnvelope");
              if (!chunk.last) return undefined;
              if (total !== m.size) throw new Refusal("badEnvelope");
              const allowed = P.MIME_MAGIC.find((x) => x.mime === m.mime);
              if (!allowed) throw new Refusal("badMagic");
              return { done: new Blob(parts, { type: allowed.mime }) };
            },
          },
        );
        return result;
      });
    },
    async disconnect() {
      // Told to the computer first, through the usual path (a fresh session if none is open, once
      // more if the session was lost). Meanwhile no racing open may save the device again.
      disconnecting = true;
      let reached = true;
      try {
        await once({ cmd: "disconnect" }, "done", true);
      } catch (e) {
        // A phone the computer already removed has nothing left to tell it.
        reached = e instanceof ConnectionError && e.state === "removed";
      }
      closed = true;
      session?.close("disconnected");
      session = null;
      await store.clear();
      set({ kind: "cantConnect" });
      return { reached };
    },
  };
  return engine;
}

const TERMINAL: ReadonlySet<string> = new Set(["done", "notSure", "nothing", "unavailable"]);

function checkClientId(id: string): void {
  try {
    checkId(id);
  } catch {
    throw new PhoneRequestError("invalid");
  }
}

function checkItemId(id: string): void {
  checkClientId(id);
}

/** The marks of a keep as the call carries them: absent when not set, so older computers parse it. */
function sent(marks: KeepMarks | undefined): {
  feeling?: Feeling;
  good?: boolean;
  capturedAt?: string;
  web?: boolean;
} {
  const out: { feeling?: Feeling; good?: boolean; capturedAt?: string; web?: boolean } = {};
  if (marks?.feeling) out.feeling = marks.feeling;
  if (marks?.good !== undefined) out.good = marks.good;
  if (marks?.capturedAt) out.capturedAt = marks.capturedAt;
  if (marks?.web) out.web = true;
  return out;
}

function checkMarks(marks: KeepMarks | undefined): void {
  if (marks?.feeling) checkFeeling(marks.feeling);
}

/** A point on the field and up to three short words, as the computer checks them. */
function checkFeeling(f: Feeling): void {
  const onField = (v: number) => Number.isFinite(v) && v >= -1 && v <= 1;
  if (!onField(f.point.pleasant) || !onField(f.point.energy)) {
    throw new PhoneRequestError("invalid");
  }
  if (f.words.length > 3) throw new PhoneRequestError("invalid");
  for (const w of f.words) {
    const n = [...w].length;
    if (n < 1 || n > 40) throw new PhoneRequestError("invalid");
  }
}

function checkDate(date: string): void {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new PhoneRequestError("invalid");
}

function checkPeriod(period: Period): void {
  checkDate(period.start);
  checkDate(period.end);
}

function checkAnswer(answer: SameNameAnswer): void {
  if (answer.type === "entity") checkClientId(answer.entityId);
}
