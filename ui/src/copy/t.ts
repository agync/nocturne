import { IntlMessageFormat } from "intl-messageformat";
import enGB from "./en-GB.json";

export type CopyId = keyof typeof enGB;
export type CopyValues = Record<string, string | number | boolean | Date | null | undefined>;

const LOCALE = "en-GB";
const compiled = new Map<string, IntlMessageFormat>();

const isTest = import.meta.env.MODE === "test";

/** Every user-visible string comes through here. An id missing from the deck never renders as the id. */
export function t(id: CopyId, values?: CopyValues): string {
  const message = (enGB as Record<string, string | undefined>)[id];
  if (message === undefined) {
    if (isTest) throw new Error(`Missing copy id: ${id}`);
    console.error(`Missing copy id: ${id}`);
    return "";
  }
  let format = compiled.get(id);
  if (!format) {
    format = new IntlMessageFormat(message, LOCALE);
    compiled.set(id, format);
  }
  const result = format.format(values ?? {});
  return Array.isArray(result) ? result.join("") : String(result);
}

export function hasCopy(id: string): id is CopyId {
  return id in enGB;
}

export const copyDeck: Readonly<Record<string, string>> = enGB;
