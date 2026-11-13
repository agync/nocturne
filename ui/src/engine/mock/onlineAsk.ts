import type { AnsweredBy, AskPhase, AskPlace, LocalReason, OnlineView } from "../bindings";

function pausedReason(view: OnlineView): LocalReason | null {
  const { state } = view;
  if (state.type === "problem") return state.code === "noInternet" ? "noInternet" : "problem";
  if (state.type !== "paused") return null;
  if (state.pause.type === "limit") return "limit";
  return state.pause.type === "keyRefused" ? "keyRefused" : "noCredit";
}

/**
 * Where the mock writes an answer. With Online answers off (or no view), every answer is written
 * on this computer, and a question the person kept here says so.
 */
export function answeredBy(place: AskPlace | undefined, online?: OnlineView | null): AnsweredBy {
  if (place === "thisComputer") return { type: "computer", reason: "chosen" };
  if (!online || online.state.type === "off" || online.state.type === "checking") {
    return { type: "computer", reason: null };
  }
  const reason = pausedReason(online);
  if (reason) return { type: "computer", reason };
  return { type: "online", host: online.host ?? "" };
}

/** The step the mock shows between searching and the answer when a question goes online. */
export function onlinePrelude(by: AnsweredBy): AskPhase[] {
  return by.type === "online" ? ["askingOnline"] : [];
}

/** Online answers set up and on, for tests and screens that need a view to read. */
export function onlineViewOn(overrides: Partial<OnlineView> = {}): OnlineView {
  return {
    state: { type: "on" },
    setUp: true,
    host: "api.example.com",
    baseUrl: "https://api.example.com/v1",
    model: "llama-3.3-70b-instruct",
    phones: true,
    phonesAvailable: false,
    sendSensitive: false,
    limit: { kind: "questions", questions: 200, usd: null },
    questionsThisMonth: 38,
    spentThisMonthUsd: null,
    costReported: false,
    resetsOn: "2026-12-01",
    lastSentAt: "2026-11-13T09:14:00Z",
    onSince: "2026-11-02T10:00:00Z",
    lastOnAt: "2026-11-13T09:14:00Z",
    sentBytes: 46_000,
    keyStorage: "keychain",
    ...overrides,
  };
}
