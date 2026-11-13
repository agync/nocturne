// Typed access to the reference vectors for the phone channel
// (`src-tauri/tests/fixtures/remote-vectors.json`, written by `scripts/remote_vectors.py`).
// Bytes in the fixture are lowercase hex. A positive vector has `inputs`, `intermediate` and
// `output`; a negative one has `inputs` and the refusal it must get in `expect`. Tests only.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export type Fields = Record<string, unknown>;

export interface Vector {
  name: string;
  step: string;
  inputs: Fields;
  intermediate: Fields;
  output: Fields;
  expect?: string;
}

export interface Reference {
  constants: Fields;
  refusals: string[];
  checkOrder: string[];
  words: { count: number; sha256: string };
  keys: Record<string, { scalar: string; public: string }>;
  positive: Vector[];
  negative: Vector[];
}

/** Tests run from `ui/`. */
const FIXTURE = resolve(process.cwd(), "../src-tauri/tests/fixtures/remote-vectors.json");

export function loadReference(): Reference {
  return JSON.parse(readFileSync(FIXTURE, "utf8")) as Reference;
}

function find(list: Vector[], name: string): Vector {
  const found = list.find((v) => v.name === name);
  if (!found) throw new Error(`no reference vector named ${name}`);
  return found;
}

export const positive = (ref: Reference, name: string): Vector => find(ref.positive, name);
export const negative = (ref: Reference, name: string): Vector => find(ref.negative, name);
export const step = (list: Vector[], name: string): Vector[] => list.filter((v) => v.step === name);

/** A field that holds hex, as bytes. */
export function bytes(fields: Fields, key: string): Uint8Array<ArrayBuffer> {
  const value = fields[key];
  if (typeof value !== "string") throw new Error(`${key} is not a hex string`);
  return hex(value);
}

export function hex(s: string): Uint8Array<ArrayBuffer> {
  if (s.length % 2 !== 0) throw new Error(`odd hex length: ${s}`);
  const out = new Uint8Array(new ArrayBuffer(s.length / 2));
  for (let i = 0; i < out.length; i += 1) out[i] = Number.parseInt(s.slice(2 * i, 2 * i + 2), 16);
  return out;
}

export function toHex(b: ArrayBuffer | Uint8Array): string {
  return Array.from(b instanceof Uint8Array ? b : new Uint8Array(b), (x) =>
    x.toString(16).padStart(2, "0"),
  ).join("");
}
