// Sends a blob as chunk frames. At most two chunks of the file are held at once: the one being
// sealed and sent, and the next one, read ahead. The chunk in hand also exists briefly as its
// envelope and its sealed frame; those are copies of that same chunk, not further chunks. A large
// file therefore never sits whole in memory.

import type { Bytes } from "./bytes";
import { copy } from "./bytes";
import * as P from "./protocol";

export interface UploadTrace {
  /** Chunks read and not yet sent: its maximum is the memory an upload held. */
  held: number;
  maxHeld: number;
}

export interface ChunkSink {
  chunk(seq: number, last: boolean, data: Bytes): Promise<void>;
}

async function read(blob: Blob, from: number, size: number, trace?: UploadTrace): Promise<Bytes> {
  const data = copy(new Uint8Array(await blob.slice(from, from + size).arrayBuffer()));
  if (trace) {
    trace.held += 1;
    trace.maxHeld = Math.max(trace.maxHeld, trace.held);
  }
  return data;
}

/** Sends every chunk in order, stopping quietly when `stop()` says the call already ended. */
export async function sendBlob(
  sink: ChunkSink,
  blob: Blob,
  stop: () => boolean,
  trace?: UploadTrace,
  size: number = P.CHUNK,
): Promise<void> {
  if (blob.size === 0) throw new Error("empty upload");
  let next: Promise<Bytes> | null = read(blob, 0, size, trace);
  for (let seq = 0, at = 0; at < blob.size; seq += 1, at += size) {
    const data = await (next as Promise<Bytes>);
    const last = at + size >= blob.size;
    next = last || stop() ? null : read(blob, at + size, size, trace);
    if (stop()) return;
    await sink.chunk(seq, last, data);
    if (trace) trace.held -= 1;
  }
}
