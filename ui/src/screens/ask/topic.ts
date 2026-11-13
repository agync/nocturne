const SKIP = new Set([
  "what",
  "when",
  "where",
  "who",
  "whom",
  "which",
  "why",
  "how",
  "the",
  "and",
  "for",
  "are",
  "was",
  "were",
  "did",
  "does",
  "has",
  "have",
  "had",
  "about",
  "with",
  "from",
  "that",
  "this",
  "still",
  "my",
  "me",
  "is",
  "it",
  "to",
  "of",
  "in",
  "on",
  "at",
  "a",
  "an",
  "do",
  "i",
  "you",
  "your",
  "our",
  "can",
  "will",
  "say",
  "said",
  "out",
  "run",
  "up",
  "off",
  "over",
  "down",
  "back",
  "again",
  "today",
  "tomorrow",
  "yesterday",
  "any",
  "there",
  "here",
  "not",
]);

/**
 * The thing a question is about, for "Search everything kept about the bakery". It is the last
 * word that is not a question word: "What did I promise the bakery?" gives "bakery".
 */
export function topicOf(question: string): string {
  const words = question
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s’'-]/gu, " ")
    .split(/\s+/)
    .map((w) => w.replace(/(’|')s$/u, ""))
    .filter((w) => w.length > 2 && !SKIP.has(w));
  return words.at(-1) ?? question.trim().replace(/\?$/, "");
}
