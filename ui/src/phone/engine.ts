// What the phone app may do, and what it keeps about itself. The allowed calls, the journal
// calls and the two phone verbs are the whole of `PhoneEngine`; nothing else crosses the sealed
// channel.

import type {
  AskDelta,
  CheckIn,
  CheckInInput,
  Feeling,
  JournalDay,
  KeptHow,
  PageSummary,
  PageView,
  Period,
  PhoneError,
  PhoneHello,
  PhoneItem,
  PhoneKept,
  PhoneStatus,
  QuestionChoice,
  ReflectionView,
  RemoteConnection,
  SameNameAnswer,
  SameNameAnswered,
} from "../engine/bindings";
import type { Bytes } from "./bytes";

/**
 * What a keep may carry besides what is kept: how it feels, that it is a good thing, and when it
 * was made (RFC 3339), so a keep that waited belongs to the day it was made, not the day it
 * arrived. The computer never puts it in the future or more than a week back.
 */
export interface KeepMarks {
  feeling?: Feeling;
  good?: boolean;
  capturedAt?: string;
  /** Text shared from a web page or a link: the computer keeps it as untrusted web content. */
  web?: boolean;
  /** How it was kept, which a phone alone notes for the thing's page. Never sent to a computer. */
  how?: KeptHow;
}

export interface PhoneEngine {
  hello(): Promise<PhoneHello>;
  status(): Promise<PhoneStatus>;
  keepText(text: string, clientId: string, marks?: KeepMarks): Promise<PhoneKept>;
  keepBytes(blob: Blob, clientId: string, caption?: string, marks?: KeepMarks): Promise<PhoneKept>;
  keepRecording(blob: Blob, clientId: string, marks?: KeepMarks): Promise<PhoneKept>;
  /** The whole check-in, by touch. Kept once however often it is sent. */
  keepCheckIn(input: CheckInInput, clientId: string, capturedAt?: string): Promise<CheckIn>;
  /** A journal day, today when none is given. The computer sends it whole; nothing is cached. */
  getJournalDay(date?: string): Promise<JournalDay>;
  /** What is written for a day or a period, or its glance. */
  getLookBack(period: Period): Promise<ReflectionView>;
  /** Asks the computer to write it ("Reflect on today"), then answers as `getLookBack` does. */
  lookBack(period: Period): Promise<ReflectionView>;
  /** "Which {name} is this?": a change a phone makes beyond keeping. */
  answerSameName(questionId: string, answer: SameNameAnswer): Promise<SameNameAnswered>;
  /**
   * The page's question from the journal: the choice, and the line when one was written before
   * the answer went across. Kept once per client id; answers with the page as it stands after.
   */
  answerJournalQuestion(
    pageDate: string,
    choice: QuestionChoice,
    clientId: string,
    line?: string,
  ): Promise<PageView>;
  unkeep(id: string): Promise<void>;
  /**
   * Fails with `onlineChanged`, starting nothing, while the question would go online and Online
   * answers' one-time notice waits. `thisComputer` keeps the question on the computer.
   */
  ask(
    question: string,
    onDelta: (delta: AskDelta) => void,
    place?: "thisComputer",
  ): Promise<string>;
  /** The person has seen that Online answers now holds back only what they mark private. */
  seeOnlineChange(): Promise<void>;
  cancelAsk(askId: string): Promise<void>;
  getPage(date?: string): Promise<PageView>;
  listPages(): Promise<PageSummary[]>;
  getItem(id: string): Promise<PhoneItem>;
  original(id: string): Promise<Blob>;
  /** Forgets this phone. `reached` is false when the computer could not be told. */
  disconnect(): Promise<{ reached: boolean }>;
}

/**
 * Where the phone stands with its computer. `removed` is only ever the result of a sealed
 * revocation; every unsealed failure leaves the keys alone.
 */
export type Connection =
  | { kind: "connecting" }
  /** `agreed`: the newest phone build both sides speak; calls and answers keep to it. */
  | { kind: "connected"; hello: PhoneHello; agreed: number }
  | { kind: "asleep" }
  | { kind: "cantConnect" }
  | { kind: "turnedOff" }
  /** This phone is older than the computer speaks; `computer` is the computer's version. */
  | { kind: "outdated"; minBuild: number; computer?: string }
  /** The computer is older than this phone speaks. */
  | { kind: "computerOutdated"; computer: string }
  | { kind: "removed" };

export interface ConnectionHandle {
  connection(): Connection;
  subscribe(listener: (c: Connection) => void): () => void;
  /** Open a session now, or find out why not. Resolves with the new state. */
  connect(): Promise<Connection>;
  /** Close this tab's session. Keys stay. */
  close(): void;
}

export type ConnectedEngine = PhoneEngine & ConnectionHandle;

/**
 * Everything the phone keeps for one pairing. Never session material. In the phone app the keys
 * stay in its native side, so only the fields the screens read are here.
 */
export interface PairedDevice {
  deviceId: Bytes;
  /** The long-term key D: not extractable, usable only to derive bits. Absent in the app. */
  privateKey?: CryptoKey;
  publicKey: Bytes;
  /** The computer's long-term public key, as scanned or answered at pairing. */
  cpub: Bytes;
  /** The origin this app was paired on; its storage lives here. */
  homeOrigin: string;
  /** The computer's current address: a host. Stored only after a session's Hello opens. */
  address: string;
  deviceName: string;
  computerName: string | null;
  connection: RemoteConnection | null;
  version: string | null;
  pairedAt: number;
  lastConnectedAt: number | null;
}

export interface KeyStore {
  device(): Promise<PairedDevice | null>;
  save(device: PairedDevice): Promise<void>;
  clear(): Promise<void>;
}

/** An answer to the page's question from the journal, waiting with the line written for it. */
export interface PendingAnswer {
  pageDate: string;
  choice: QuestionChoice;
  line: string | null;
}

export interface PendingKeep {
  clientId: string;
  kind: "text" | "photo" | "recording" | "checkin" | "answer";
  text?: string;
  blob?: Blob;
  caption?: string;
  /** How it feels, kept with the keep so a keep that waits does not lose it. */
  feeling?: Feeling;
  good?: boolean;
  /** Shared from a web page or a link. */
  web?: boolean;
  /** How it was kept, for its page on a phone alone. */
  how?: KeptHow;
  /** A check-in kept while the computer was out of reach. */
  checkIn?: CheckInInput;
  /** An answer to the page's question given while the computer was out of reach. */
  answer?: PendingAnswer;
  /** Bytes of the text, or the blob's size; a check-in counts its words. */
  size: number;
  createdAt: number;
}

export interface FlushResult {
  sent: number;
  failed: number;
  remaining: number;
  kept: PhoneKept[];
  checkIns: CheckIn[];
}

export interface Queue {
  add(keep: PendingKeep): Promise<void>;
  /** Forget one keep that the computer has, or will never take. */
  remove(clientId: string): Promise<void>;
  count(): Promise<number>;
  list(): Promise<PendingKeep[]>;
  flush(engine: PhoneEngine): Promise<FlushResult>;
  clear(): Promise<void>;
}

/** A call the computer answered with an error, or one the phone refused to send. */
export class PhoneRequestError extends Error {
  readonly code: PhoneError;
  constructor(code: PhoneError) {
    super(code);
    this.name = "PhoneRequestError";
    this.code = code;
  }
}

/** The path to the computer failed or the session ended. The call may be tried again. */
export class ConnectionError extends Error {
  readonly state: Connection["kind"];
  readonly status: number | undefined;
  constructor(state: Connection["kind"], status?: number) {
    super(state);
    this.name = "ConnectionError";
    this.state = state;
    this.status = status;
  }
}
