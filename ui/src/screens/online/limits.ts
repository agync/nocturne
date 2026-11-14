import { t } from "../../copy/t";
import type { OnlineLimit } from "../../engine/bindings";

/** Dollars as the provider reports them: "$5.00". */
export const usdText = (usd: number): string => `$${usd.toFixed(2)}`;

export const QUESTION_STEPS = [50, 200, 500] as const;
export const USD_STEPS = [5, 10, 20] as const;

/** What a new setup starts with: no limit, and 200 as the number offered if one is set. */
export const DEFAULT_LIMIT: OnlineLimit = { kind: "none", questions: 200, usd: null };

/** "200 questions", "$5.00 a month" or "No limit". */
export function limitText(limit: OnlineLimit): string {
  if (limit.kind === "none") return t("online.limit.option.none");
  if (limit.kind === "usd")
    return t("online.limit.option.usd", { amount: usdText(limit.usd ?? 0) });
  return t("online.limit.option.questions", { count: limit.questions });
}

/** A whole number of questions from 1 upwards, or null. */
export function parseQuestions(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const n = Number(trimmed);
  return Number.isSafeInteger(n) && n >= 1 ? n : null;
}

/** An amount in dollars above zero, with at most two decimals, or null. */
export function parseUsd(text: string): number | null {
  const trimmed = text.trim().replace(/^\$/, "");
  if (!/^\d+(\.\d{1,2})?$/.test(trimmed)) return null;
  const n = Number(trimmed);
  return n > 0 ? n : null;
}
