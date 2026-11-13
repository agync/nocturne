// What the phone app may do, and what it keeps about itself. The ten allowed calls and the two
// phone verbs are the whole of `PhoneEngine`; nothing else crosses the sealed channel.

import type {
  AskDelta,
  PageSummary,
  PageView,
  PhoneError,
  PhoneHello,
  PhoneItem,
  PhoneKept,
  PhoneStatus,
  RemoteConnection,
} from "../engine/bindings";
import type { Bytes } from "./bytes";

export interface PhoneEngine {
  hello(): Promise<PhoneHello>;
  status(): Promise<PhoneStatus>;
  keepText(text: string, clientId: string): Promise<PhoneKept>;
  keepBytes(blob: Blob, clientId: string, caption?: string): Promise<PhoneKept>;
  keepRecording(blob: Blob, clientId: string): Promise<PhoneKept>;
  unkeep(id: string): Promise<void>;
  ask(question: string, onDelta: (delta: AskDelta) => void): Promise<string>;
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
  | { kind: "connected"; hello: PhoneHello }
  | { kind: "asleep" }
  | { kind: "cantConnect" }
  | { kind: "turnedOff" }
  | { kind: "outdated"; minBuild: number }
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

/** Everything the phone keeps for one pairing. Never session material. */
export interface PairedDevice {
  deviceId: Bytes;
  /** The long-term key D: not extractable, usable only to derive bits. */
  privateKey: CryptoKey;
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

export interface PendingKeep {
  clientId: string;
  kind: "text" | "photo" | "recording";
  text?: string;
  blob?: Blob;
  caption?: string;
  /** Bytes of the text, or the blob's size. */
  size: number;
  createdAt: number;
}

export interface FlushResult {
  sent: number;
  failed: number;
  remaining: number;
  kept: PhoneKept[];
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
