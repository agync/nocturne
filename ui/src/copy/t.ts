import { IntlMessageFormat } from "intl-messageformat";
import { enGB } from "./deck";

export type CopyId = keyof typeof enGB;
export type CopyValues = Record<string, string | number | boolean | Date | null | undefined>;

const LOCALE = "en-GB";
const compiled = new Map<string, IntlMessageFormat>();

const isTest = import.meta.env.MODE === "test";

function compile(id: CopyId): IntlMessageFormat | undefined {
  const message = (enGB as Record<string, string | undefined>)[id];
  if (message === undefined) {
    if (isTest) throw new Error(`Missing copy id: ${id}`);
    console.error(`Missing copy id: ${id}`);
    return undefined;
  }
  let format = compiled.get(id);
  if (!format) {
    format = new IntlMessageFormat(message, LOCALE);
    compiled.set(id, format);
  }
  return format;
}

/** Every user-visible string comes through here. An id missing from the deck never renders as the id. */
export function t(id: CopyId, values?: CopyValues): string {
  const result = compile(id)?.format(values ?? {}) ?? "";
  return Array.isArray(result) ? result.join("") : String(result);
}

/**
 * The same string with arguments that are not text, such as an element drawn in the sentence.
 * Returns the text and the arguments in order.
 */
export function tParts(id: CopyId, values: Record<string, unknown>): unknown[] {
  const result = compile(id)?.format(values as never) ?? "";
  return Array.isArray(result) ? result : [result];
}

export function hasCopy(id: string): id is CopyId {
  return id in enGB;
}

export const copyDeck: Readonly<Record<string, string>> = enGB;
