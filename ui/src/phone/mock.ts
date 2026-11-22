// A scripted phone engine, for the development build and the end-to-end tests: the same
// interface as the sealed one, answering from a script with no computer and no network.

import type {
  AskDelta,
  CheckIn,
  PageLine,
  PageView,
  PhoneHello,
  PhoneItem,
  PhoneKept,
  Question,
  QuestionChoice,
  Receipt,
  ReflectionView,
} from "../engine/bindings";
import { copy } from "./bytes";
import {
  type ConnectedEngine,
  type Connection,
  ConnectionError,
  PhoneRequestError,
} from "./engine";
import {
  emptyDay,
  lookBackFor,
  SAME_NAME,
  TODAY,
  todayDay,
  todayReflection,
  YESTERDAY,
  yesterdayDay,
} from "./testing/journalFixtures";

export interface MockScript {
  connection?: Connection["kind"];
  hello?: Partial<PhoneHello>;
  /** Milliseconds between streamed pieces of an answer. */
  pace?: number;
  /** No page has been written yet. */
  noPage?: boolean;
  /** Keeps fail as if the computer went to sleep mid-way. */
  keepFails?: boolean;
  /** The computer answered the page's question with Fine before the phone's answer arrived. */
  answeredElsewhere?: boolean;
  /** The computer refuses an answer to the page's question. */
  answerRefused?: boolean;
  /** Online answers' one-time notice waits: a question that would go online is not started. */
  onlineChanged?: boolean;
  /** Saving that the notice was seen fails. */
  seeFails?: boolean;
  now?: () => number;
}

const PNG = Uint8Array.from(
  atob(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  ),
  (c) => c.charCodeAt(0),
);

function wav(seconds: number): Uint8Array {
  const rate = 8000;
  const samples = rate * seconds;
  const out = new Uint8Array(44 + samples);
  const view = new DataView(out.buffer);
  const text = (at: number, s: string) => {
    for (const [i, c] of [...s].entries()) out.set([c.charCodeAt(0)], at + i);
  };
  text(0, "RIFF");
  view.setUint32(4, 36 + samples, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate, true);
  view.setUint16(32, 1, true);
  view.setUint16(34, 8, true);
  text(36, "data");
  view.setUint32(40, samples, true);
  out.fill(128, 44);
  return out;
}

const day = (d: string) => `${d}T10:00:00.000Z`;

function receipt(
  itemId: string,
  kind: Receipt["kind"],
  label: string,
  keptAt: string,
  extra: Partial<Receipt> = {},
): Receipt {
  return {
    itemId,
    kind,
    source: "drop",
    label,
    keptAt: day(keptAt),
    durationMs: null,
    span: null,
    unconfirmed: false,
    fromDevice: null,
    ...extra,
  };
}

export const MOCK_RECEIPTS = {
  letter: receipt("item-letter", "image", "Year 4 trip letter", "2026-11-11"),
  chat: receipt("item-chat", "text", "Class WhatsApp, 3 messages", "2026-11-12", {
    source: "paste",
    span: { type: "quote", text: "Packed lunch in a bag they can throw away please!" },
  }),
  card: receipt("item-card", "image", "Appointment card", "2026-11-03"),
  voice: receipt("item-voice", "audio", "Dentist moved", "2026-11-10", {
    source: "recording",
    durationMs: 8000,
  }),
};

const ITEMS: Record<string, { item: PhoneItem; blob: Blob | null }> = {
  "item-letter": {
    item: {
      id: "item-letter",
      kind: "image",
      title: "Year 4 trip letter",
      keptAt: day("2026-11-11"),
      durationMs: null,
      pages: null,
      hasOriginal: true,
      summary: null,
      text: null,
      truncated: false,
    },
    blob: new Blob([copy(PNG)], { type: "image/png" }),
  },
  "item-card": {
    item: {
      id: "item-card",
      kind: "image",
      title: "Appointment card",
      keptAt: day("2026-11-03"),
      durationMs: null,
      pages: null,
      hasOriginal: true,
      summary: null,
      text: null,
      truncated: false,
    },
    blob: new Blob([copy(PNG)], { type: "image/png" }),
  },
  "item-chat": {
    item: {
      id: "item-chat",
      kind: "text",
      title: "Class WhatsApp, 3 messages",
      keptAt: day("2026-11-12"),
      durationMs: null,
      pages: null,
      hasOriginal: false,
      summary: null,
      text: "Class rep, 19:02: Reminder: Science Museum trip is Friday 21st. Packed lunch in a bag they can throw away please!\nPriya, 19:10: Coach leaves at 8:45 sharp, so drop-off is 8:30.\nClass rep, 19:12: Yes, 8:45. Slips and £12 to the office by this Friday.",
      truncated: false,
    },
    blob: null,
  },
  "item-voice": {
    item: {
      id: "item-voice",
      kind: "audio",
      title: "Dentist moved",
      keptAt: day("2026-11-10"),
      durationMs: 8000,
      pages: null,
      hasOriginal: true,
      summary: null,
      text: "Willow Dental rang. Leo’s check-up has moved to Thursday the twentieth, at ten.",
      truncated: false,
    },
    blob: new Blob([copy(wav(8))], { type: "audio/wav" }),
  },
};

const lines = (items: Array<[string, string, Receipt[], PageLine["kind"]?]>): PageLine[] =>
  items.map(([id, text, receipts, kind]) => ({
    id,
    kind: kind ?? "changed",
    text,
    memoryIds: [],
    receipts,
    choices: [],
    conflictId: null,
    subject: null,
    done: false,
    forgotten: false,
    on: null,
    lastUsed: null,
  }));

/** The page of the 13th asks about a plan written in the entry of the 12th. */
export const QUESTION_PAGE = "2026-11-13";
export const QUESTION_TEXT = "Parents’ evening at six. Not looking forward to it.";

export interface AnswerState {
  choice: QuestionChoice | null;
  line: string | null;
  clients: Set<string>;
}

export function journalQuestion(answer: AnswerState): Question {
  return {
    kind: "journal",
    text: QUESTION_TEXT,
    memoryId: "mem-parents-evening",
    receipt: receipt("item-entry-12", "text", "Entry", "2026-11-12", {
      source: "journal",
      keptAt: "2026-11-12T20:12:00.000Z",
    }),
    lastAsked: null,
    answered: answer.choice,
    journal: {
      itemId: "item-entry-12",
      voice: false,
      writtenOn: "2026-11-12",
      eventDay: "2026-11-12",
      answerItemId: answer.choice ? "item-answer" : null,
      line: answer.line,
    },
  };
}

export function page(date: string, headline: string): PageView {
  return {
    date,
    pageNumber: 42,
    edition: "phone",
    variant: "normal",
    headline,
    dek: "Overnight Nocturne read 14 things you kept, noticed 3 changes and let 3 go.",
    needsYou: lines([
      [
        "n1",
        "The Year 4 trip form is due **Friday**. It needs a signature and £12.",
        [MOCK_RECEIPTS.letter],
        "due",
      ],
      [
        "n2",
        "Your dentist appointment changed. Tuesday 18 Nov, 14:00 or Thursday 20 Nov, 10:00?",
        [MOCK_RECEIPTS.card, MOCK_RECEIPTS.voice],
        "which",
      ],
    ]),
    changed: lines([
      ["c1", "Leo’s swimming moved from Wednesday to **Monday**, same time.", [MOCK_RECEIPTS.chat]],
      [
        "c2",
        "You decided to book the **Lake District** cottage for half term.",
        [MOCK_RECEIPTS.voice],
      ],
    ]),
    letGo: [],
    upcoming: [],
    learned: [],
    question: null,
    moreNeedsYou: 0,
    moreChanged: 0,
    keptCount: 14,
    changesCount: 3,
    letGoCount: 3,
    catchUpDays: 0,
    privateSkipped: 0,
    pending: 0,
    tidiedFrom: null,
    tidiedTo: null,
    update: null,
  };
}

const delta = (
  phase: AskDelta["phase"],
  textDelta: string | null,
  extra: Partial<AskDelta> = {},
): AskDelta => ({
  askId: "ask-1",
  phase,
  textDelta,
  receipts: [],
  memoryIds: [],
  near: [],
  found: null,
  etaSeconds: null,
  plain: false,
  ...extra,
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function createMockPhoneEngine(script: MockScript = {}): ConnectedEngine {
  const state: Connection = initial(script);
  const listeners = new Set<(c: Connection) => void>();
  const pace = script.pace ?? 120;
  const now = script.now ?? (() => Date.now());
  const gate = () => {
    if (state.kind !== "connected")
      throw new ConnectionError(state.kind === "removed" ? "removed" : "asleep");
  };
  let stopped = false;
  let kept = 0;
  let noticeDue = script.onlineChanged ?? false;
  const answer: AnswerState = { choice: null, line: null, clients: new Set() };
  const pageOf = (date: string | undefined): PageView => {
    if (date === "2026-11-12") return page("2026-11-12", "A quiet Wednesday.");
    return {
      ...page(QUESTION_PAGE, "Two things need you today."),
      question: journalQuestion(answer),
    };
  };

  const keptAnswer = (title: string, kind: PhoneItem["kind"]): PhoneKept => {
    if (script.keepFails) throw new ConnectionError("asleep");
    gate();
    kept += 1;
    return {
      item: {
        id: `item-new-${kept}`,
        kind,
        title,
        keptAt: new Date(now()).toISOString(),
        durationMs: null,
        pages: null,
        hasOriginal: kind !== "text",
        summary: null,
        text: null,
        truncated: false,
      },
      duplicate: false,
      takeBackUntil: new Date(now() + 600_000).toISOString(),
    };
  };

  return {
    connection: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async connect() {
      return state;
    },
    close() {},
    async hello() {
      gate();
      return helloOf(script);
    },
    async status() {
      gate();
      return { readiness: "ready", pageDate: "2026-11-13", needsYou: 2 };
    },
    async keepText(text) {
      return keptAnswer(text.slice(0, 40), "text");
    },
    async keepBytes(_blob, _id, caption) {
      return keptAnswer(caption?.trim() || "Photo", "image");
    },
    async keepRecording() {
      return keptAnswer("Voice note", "audio");
    },
    async keepCheckIn(input) {
      if (script.keepFails) throw new ConnectionError("asleep");
      gate();
      if (input.replaces) throw new PhoneRequestError("invalid");
      kept += 1;
      const checkin: CheckIn = {
        id: `checkin-new-${kept}`,
        keptAt: new Date(now()).toISOString(),
        day: TODAY,
        feeling: input.feeling,
        line: input.line,
        urge: input.urge,
        about: null,
        tried: [],
        fromDevice: null,
        mentions: [],
        leftOut: false,
      };
      return checkin;
    },
    async getJournalDay(date) {
      gate();
      const wanted = date ?? TODAY;
      if (wanted > TODAY) throw new PhoneRequestError("invalid");
      if (wanted === TODAY) return todayDay();
      return wanted === YESTERDAY ? yesterdayDay() : emptyDay(wanted, false);
    },
    async getLookBack(period) {
      gate();
      return lookBackFor(period);
    },
    async lookBack(period) {
      gate();
      await sleep(pace);
      if (period.kind === "day" && period.start === TODAY) {
        const view: ReflectionView = {
          type: "written",
          reflection: todayReflection(new Date(now()).toISOString()),
        };
        return view;
      }
      return lookBackFor(period);
    },
    async answerSameName(questionId, answer) {
      gate();
      if (questionId !== SAME_NAME.id) throw new PhoneRequestError("notFound");
      const chosen =
        answer.type === "entity"
          ? SAME_NAME.options.find((o) => o.entityId === answer.entityId)
          : undefined;
      return { name: SAME_NAME.name, role: chosen?.role ?? null };
    },
    async unkeep() {
      gate();
    },
    async ask(question, onDelta, place) {
      gate();
      stopped = false;
      const q = question.toLowerCase();
      if (q.includes("too many")) throw new PhoneRequestError("tooMany");
      if (noticeDue && place !== "thisComputer") throw new PhoneRequestError("onlineChanged");
      const emit = async (d: AskDelta) => {
        if (stopped) return;
        onDelta(d);
        await sleep(pace);
      };
      await emit(delta("searching", null));
      if (q.includes("dentist")) {
        await emit(
          delta("notSure", null, {
            near: [
              { text: "Tuesday 18 Nov, 14:00", receipt: MOCK_RECEIPTS.card },
              { text: "Thursday 20 Nov, 10:00", receipt: MOCK_RECEIPTS.voice },
            ],
          }),
        );
      } else if (q.includes("nothing")) {
        await emit(delta("nothing", null));
      } else {
        await emit(
          delta(
            "streaming",
            "Leo’s Year 4 trip to the Science Museum is on **Friday 21 November**. ",
          ),
        );
        await emit(
          delta(
            "streaming",
            "The signed slip and £12 are due tomorrow. Bring a packed lunch in a bag you won’t need back.",
          ),
        );
        const answeredBy: AskDelta["answeredBy"] =
          place === "thisComputer"
            ? { type: "computer", reason: "chosen" }
            : q.includes("online") || script.onlineChanged
              ? { type: "online", host: "api.example.test" }
              : q.includes("on my computer")
                ? { type: "computer", reason: null }
                : null;
        await emit(
          delta("done", null, { receipts: [MOCK_RECEIPTS.letter, MOCK_RECEIPTS.chat], answeredBy }),
        );
      }
      return "ask-1";
    },
    async cancelAsk() {
      stopped = true;
    },
    async seeOnlineChange() {
      gate();
      await sleep(pace);
      if (script.seeFails) throw new PhoneRequestError("notReady");
      noticeDue = false;
    },
    async getPage(date) {
      gate();
      if (script.noPage) throw new PhoneRequestError("notFound");
      return pageOf(date);
    },
    async answerJournalQuestion(pageDate, choice, clientId, line) {
      if (script.keepFails) throw new ConnectionError("asleep");
      gate();
      if (script.answerRefused) throw new PhoneRequestError("invalid");
      if (pageDate !== QUESTION_PAGE) throw new PhoneRequestError("notFound");
      if (script.answeredElsewhere && answer.choice === null) answer.choice = "fine";
      if (!answer.clients.has(clientId)) {
        answer.clients.add(clientId);
        answer.choice ??= choice;
      }
      const said = line?.trim();
      if (said && answer.choice === choice && answer.line === null) answer.line = said;
      return pageOf(pageDate);
    },
    async listPages() {
      gate();
      return [
        { date: "2026-11-12", pageNumber: 41, headline: "A quiet Wednesday.", needsYou: 0 },
        { date: "2026-11-11", pageNumber: 40, headline: "One thing needs you.", needsYou: 1 },
        { date: "2026-11-10", pageNumber: 39, headline: "A new week.", needsYou: 0 },
      ];
    },
    async getItem(id) {
      gate();
      const found = ITEMS[id];
      if (!found) throw new PhoneRequestError(id === "item-private" ? "notYours" : "notFound");
      return found.item;
    },
    async original(id) {
      gate();
      const blob = ITEMS[id]?.blob;
      if (!blob) throw new PhoneRequestError("notFound");
      return blob;
    },
    async disconnect() {
      return { reached: true };
    },
  };
}

function helloOf(script: MockScript): PhoneHello {
  return {
    computerName: "Home computer",
    deviceName: "Pixel",
    connection: "simple",
    version: "0.1.0",
    minPhoneBuild: 1,
    ...script.hello,
  };
}

function initialConnected(script: MockScript): Connection {
  return { kind: "connected", hello: helloOf(script) };
}

function initial(script: MockScript): Connection {
  const kind = script.connection ?? "connected";
  if (kind === "connected") return initialConnected(script);
  if (kind === "outdated") return { kind, minBuild: 2 };
  return { kind } as Connection;
}
