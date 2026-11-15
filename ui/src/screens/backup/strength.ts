import type { CopyId } from "../../copy/t";

export type StrengthLevel = "tooShort" | "weak" | "fair" | "strong";

export interface Strength {
  level: StrengthLevel;
  /** Filled marks out of three. None for a passphrase under eight characters. */
  marks: 0 | 1 | 2 | 3;
  copy: CopyId;
}

/**
 * Passphrases so common that a guesser tries them first. Whole phrases and single words, kept
 * small on purpose: this is advice on one computer, not a dictionary.
 */
const COMMON = new Set([
  "12345678",
  "123456789",
  "1234567890",
  "password",
  "password1",
  "passphrase",
  "passw0rd",
  "qwertyui",
  "qwerty123",
  "letmein",
  "iloveyou",
  "welcome",
  "welcome1",
  "admin",
  "changeme",
  "secret",
  "sunshine",
  "monkey",
  "dragon",
  "football",
  "princess",
  "abc12345",
  "correct horse battery staple",
]);

const WORD = /^[\p{L}]+$/u;

/** Characters, not UTF-16 units, so an emoji counts once. */
export const lengthOf = (passphrase: string): number => [...passphrase].length;

/**
 * How strong a passphrase looks on this computer, from the design's table: under eight characters,
 * weak (common, one word, or under twelve), fair, or strong. Advice, never a gate beyond the
 * minimum length, and the passphrase is not stored anywhere.
 */
export function strengthOf(passphrase: string): Strength {
  if (lengthOf(passphrase) < 8) {
    return { level: "tooShort", marks: 0, copy: "backup.strength.tooShort" };
  }
  const text = passphrase.normalize("NFC").trim().toLocaleLowerCase("en-GB");
  const length = lengthOf(text);
  const words = text.split(/[\s-]+/u).filter(Boolean);
  const realWords = words.filter((word) => lengthOf(word) >= 2 && /\p{L}/u.test(word)).length;
  const common = COMMON.has(text) || words.some((word) => COMMON.has(word));
  const oneWord = words.length <= 1 && WORD.test(text);
  if (length < 12 || common || oneWord) {
    return { level: "weak", marks: 1, copy: "backup.strength.weak" };
  }
  // A few words that do not go together are what makes a passphrase strong.
  if (realWords >= 3 && length >= 14) {
    return { level: "strong", marks: 3, copy: "backup.strength.strong" };
  }
  if (length >= 18 && words.length >= 2) {
    return { level: "strong", marks: 3, copy: "backup.strength.strong" };
  }
  if (words.length <= 1 && length >= 16) {
    return { level: "strong", marks: 3, copy: "backup.strength.strong" };
  }
  if (length >= 20) {
    return { level: "strong", marks: 3, copy: "backup.strength.strong" };
  }
  return { level: "fair", marks: 2, copy: "backup.strength.fair" };
}
