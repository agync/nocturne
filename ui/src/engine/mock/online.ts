import type {
  CheckOutcome,
  OnlineAdvanced,
  OnlineForm,
  OnlineSend,
  OnlineSettings,
  OnlineSettingsPatch,
  OnlineState,
  OnlineView,
  PrivacyView,
  SentLine,
} from "../bindings";
import { EngineError } from "../client";

/** The states a visit can start from: `?mockOnline=<name>` before the `#`. */
export const ONLINE_SCENARIOS = [
  "off",
  "offBefore",
  "on",
  "cost",
  "limit",
  "refused",
  "noCredit",
  "problem",
  "keyUnavailable",
  "checking",
  "changed",
] as const;
export type OnlineScenario = (typeof ONLINE_SCENARIOS)[number];

const HOST = "api.example.com";
const BASE_URL = "https://api.example.com/v1";
const MODEL = "llama-3.3-70b-instruct";
const DAY = 86_400_000;

const clone = <T>(value: T): T => structuredClone(value);

const dayOf = (at: number) => {
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

/** The first day of next month, as `YYYY-MM-DD`. */
const nextMonth = (now: number) => {
  const d = new Date(now);
  return dayOf(new Date(d.getFullYear(), d.getMonth() + 1, 1, 12).getTime());
};

export interface OnlineOptions {
  /** The state to start in; see `ONLINE_SCENARIOS`. */
  scenario?: string;
  /** How long a connection check takes, in milliseconds. */
  checkMs?: number;
  /** Tells the shell something changed, as `online-changed` does. */
  onChange?: (view: OnlineView) => void;
}

/** `?mockOnline=<state>` and `?mockCheckMs=<ms>` start Online answers elsewhere than off. */
function optionsFromAddress(): OnlineOptions {
  if (typeof location === "undefined") return {};
  const params = new URLSearchParams(location.search);
  const ms = Number(params.get("mockCheckMs"));
  return {
    scenario: params.get("mockOnline") ?? undefined,
    checkMs: params.has("mockCheckMs") && ms >= 0 ? ms : undefined,
  };
}

interface Pending {
  settle: (outcome: CheckOutcome) => void;
  timer: ReturnType<typeof setTimeout>;
}

/**
 * Online answers in the mock. The check reads the form the way a person would test it: a key
 * with `refused`, `nocredit`, `busy`, `broken` or `empty` in it, a model name with `missing`, an
 * address with `offline` or `unreachable` in its host, or one that does not start with https,
 * each give that outcome. Anything else works. A test can also call `forceOutcome`.
 */
export class OnlineSim {
  private current: OnlineView;
  private checkMs: number;
  private pending: Pending | null = null;
  private readonly log: OnlineSend[] = [];
  private provisional: { host: string; baseUrl: string; model: string } | null = null;
  private forced: CheckOutcome | null = null;
  private readonly onChange?: (view: OnlineView) => void;
  private readonly now: () => number;
  /** Every form a check was asked about, without its key. For tests. */
  readonly checks: Array<{ baseUrl: string; model: string }> = [];
  /** How many times setup was left. For tests. */
  abandoned = 0;

  constructor(options: OnlineOptions = {}, now: () => number = Date.now) {
    const merged = { ...optionsFromAddress(), ...options };
    this.now = now;
    this.checkMs = merged.checkMs ?? 120;
    this.onChange = merged.onChange;
    this.current = this.seed(merged.scenario ?? "off");
  }

  /** Whether the person has seen that only what they mark private is held back. */
  private seen = true;

  private seed(scenario: string): OnlineView {
    const now = this.now();
    const base: OnlineView = {
      state: { type: "off" },
      setUp: false,
      host: null,
      baseUrl: null,
      model: null,
      phones: false,
      phonesAvailable: false,
      sendSensitive: false,
      changeNotice: false,
      limit: { kind: "none", questions: 200, usd: null },
      questionsThisMonth: 0,
      spentThisMonthUsd: null,
      costReported: false,
      resetsOn: nextMonth(now),
      lastSentAt: null,
      onSince: null,
      lastOnAt: null,
      sentBytes: 0,
      keyStorage: null,
    };
    const connected: Partial<OnlineView> = {
      setUp: true,
      host: HOST,
      baseUrl: BASE_URL,
      model: MODEL,
      keyStorage: "keychain",
      phones: true,
      phonesAvailable: true,
      limit: { kind: "questions", questions: 200, usd: null },
      sentBytes: 46_000,
      questionsThisMonth: 38,
      lastSentAt: new Date(now - 3_600_000).toISOString(),
      onSince: new Date(now - 11 * DAY).toISOString(),
    };
    const on: OnlineView = { ...base, ...connected, state: { type: "on" } };
    const paused = (pause: Extract<OnlineState, { type: "paused" }>["pause"]): OnlineView => ({
      ...on,
      state: { type: "paused", pause },
    });
    // A fresh computer has not seen the words yet: its consent step records them.
    this.seen = scenario !== "changed" && scenario !== "off";
    switch (scenario) {
      case "changed":
        return on;
      case "offBefore":
        return {
          ...base,
          ...connected,
          onSince: null,
          lastOnAt: new Date(now - 1.2 * DAY).toISOString(),
        };
      case "on":
        return on;
      case "cost":
        return { ...on, costReported: true, spentThisMonthUsd: 0.42 };
      case "limit":
        return {
          ...paused({ type: "limit", until: nextMonth(now) }),
          questionsThisMonth: 200,
          lastSentAt: new Date(now - DAY).toISOString(),
        };
      case "refused":
        return paused({ type: "keyRefused" });
      case "noCredit":
        return paused({ type: "noCredit" });
      case "problem":
        return {
          ...on,
          state: {
            type: "problem",
            code: "unreachable",
            since: new Date(now - 3_600_000).toISOString(),
          },
        };
      case "checking":
        return { ...base, state: { type: "checking" } };
      case "keyUnavailable":
        return {
          ...on,
          state: { type: "problem", code: "keyUnavailable", since: new Date(now).toISOString() },
        };
      default:
        return base;
    }
  }

  /** Puts the mock in a named state, as opening the page with `?mockOnline=` does. */
  setScenario(scenario: OnlineScenario): void {
    this.current = this.seed(scenario);
    this.changed();
  }

  setCheckMs(ms: number): void {
    this.checkMs = ms;
  }

  /** The next check answers this, whatever the form says. */
  forceOutcome(outcome: CheckOutcome): void {
    this.forced = outcome;
  }

  private changed(): void {
    this.onChange?.(clone(this.current));
  }

  view(): OnlineView {
    return {
      ...clone(this.current),
      changeNotice: this.current.state.type !== "off" && !this.seen,
    };
  }

  /** The person has seen that only what they mark private is held back. */
  async seeChange(): Promise<OnlineView> {
    this.seen = true;
    this.changed();
    return this.view();
  }

  privacy(): Pick<
    PrivacyView,
    "sentOnlineBytes" | "onlineQuestions" | "onlineOn" | "onlineHost" | "onlineLastOnAt"
  > {
    const v = this.current;
    return {
      sentOnlineBytes: v.sentBytes,
      onlineQuestions: v.questionsThisMonth,
      onlineOn: v.state.type === "on" || v.state.type === "paused" || v.state.type === "problem",
      onlineHost: v.host,
      onlineLastOnAt: v.lastOnAt ?? v.onSince,
    };
  }

  advanced(): OnlineAdvanced | null {
    const v = this.current;
    if (!v.setUp) return null;
    const last = this.log.find((s) => s.kind === "question");
    return {
      state: v.state.type,
      route: "127.0.0.1:8020",
      baseUrl: v.baseUrl,
      host: v.host,
      model: v.model,
      maxTokens: 600,
      keyStorage: v.keyStorage,
      lastAt: last?.at ?? null,
      lastStatus: last ? 200 : null,
      lastSeconds: last ? 1.2 : null,
      lastBytesUp: last?.bytesUp ?? null,
      lastBytesDown: last?.bytesDown ?? null,
      costReported: v.costReported,
      sendsKept: this.log.length,
      questionsThisMonth: v.questionsThisMonth,
    };
  }

  /**
   * Notes one question for the sent log, with the sentences it really carried. A question with
   * nothing to send is noted as not sent. Returns the entry's id.
   */
  record(question: string, ending: string, facts: string[]): number {
    const id = this.log.length + 1;
    const sent = facts.length > 0;
    this.log.unshift({
      id,
      at: new Date(this.now()).toISOString(),
      kind: "question",
      host: this.current.host ?? HOST,
      model: this.current.model ?? MODEL,
      question,
      lines: sent
        ? [
            { type: "text", text: "Facts:" },
            ...facts.map(
              (text, i): SentLine => ({ type: "memory", n: i + 1, text, state: "same" }),
            ),
            { type: "text", text: `Question: ${question}` },
          ]
        : [],
      instructions: sent ? "Answer only from the facts." : "",
      bytesUp: sent ? 1400 : 0,
      bytesDown: sent ? 600 : 0,
      costUsd: null,
      // Reached only when bytes went up: a failed entry with nothing sent never left.
      reached: sent,
      result: !sent
        ? { type: "notSent", reason: "keptHere" }
        : ending === "notSure"
          ? { type: "notSure" }
          : { type: "answered", receipts: facts.length },
    });
    return id;
  }

  private outcomeFor(form: OnlineForm): CheckOutcome {
    if (this.forced) {
      const forced = this.forced;
      this.forced = null;
      return forced;
    }
    const url = form.baseUrl.trim();
    const key = form.key.toLowerCase();
    const model = form.model.trim();
    let parsed: URL;
    try {
      parsed = new URL(url);
    } catch {
      return { type: "unreachable" };
    }
    const host = parsed.host;
    const where = reach(parsed.hostname);
    if (where === "never" || parsed.username || parsed.password || parsed.search || parsed.hash) {
      return { type: "addressNotAllowed" };
    }
    const plain = parsed.protocol === "http:" && where === "own";
    if (parsed.protocol !== "https:" && !plain) return { type: "notHttps" };
    if (/offline/.test(host)) return { type: "noInternet" };
    if (/unreachable/.test(host)) return { type: "unreachable" };
    if (key.includes("refused")) return { type: "keyRefused" };
    if (key.includes("nocredit")) return { type: "noCredit" };
    if (key.includes("busy")) return { type: "busy" };
    if (key.includes("broken")) return { type: "serverError" };
    if (key.includes("empty")) return { type: "emptyReply" };
    if (model.includes("missing")) return { type: "modelMissing" };
    return { type: "works", host, model };
  }

  async check(form: OnlineForm): Promise<CheckOutcome> {
    if (this.pending) throw new EngineError("busy", "a check is already running");
    this.checks.push({ baseUrl: form.baseUrl, model: form.model });
    const previous = this.current.state;
    this.current = { ...this.current, state: { type: "checking" } };
    this.changed();
    const outcome = this.outcomeFor(form);
    const result = await new Promise<CheckOutcome>((resolve) => {
      const timer = setTimeout(() => resolve(outcome), this.checkMs);
      this.pending = { settle: resolve, timer };
    });
    this.pending = null;
    this.current = { ...this.current, state: previous };
    this.provisional =
      result.type === "works"
        ? { host: result.host, baseUrl: form.baseUrl.trim(), model: result.model }
        : null;
    this.changed();
    return result;
  }

  async cancelCheck(): Promise<void> {
    const pending = this.pending;
    if (!pending) return;
    clearTimeout(pending.timer);
    pending.settle({ type: "cancelled" });
  }

  async turnOn(settings: OnlineSettings): Promise<OnlineView> {
    const route = this.provisional ?? (this.current.setUp ? this.current : null);
    if (!route?.host || !route.baseUrl || !route.model) {
      throw new EngineError("invalid", "check the connection first");
    }
    const was = this.current;
    this.current = {
      ...was,
      state: { type: "on" },
      setUp: true,
      host: route.host,
      baseUrl: route.baseUrl,
      model: route.model,
      phones: settings.phones,
      sendSensitive: settings.sendSensitive,
      limit: clone(settings.limit),
      keyStorage: was.keyStorage ?? "keychain",
      onSince:
        was.state.type !== "off" && was.onSince ? was.onSince : new Date(this.now()).toISOString(),
    };
    this.provisional = null;
    this.changed();
    return this.view();
  }

  async turnOff(forget: boolean): Promise<OnlineView> {
    const was = this.current;
    const wasOff = was.state.type === "off";
    this.current = {
      ...was,
      state: { type: "off" },
      onSince: null,
      lastOnAt: wasOff ? was.lastOnAt : new Date(this.now()).toISOString(),
      ...(forget ? { setUp: false, host: null, baseUrl: null, model: null, keyStorage: null } : {}),
    };
    this.changed();
    return this.view();
  }

  async abandonSetup(): Promise<void> {
    this.abandoned += 1;
    this.provisional = null;
    await this.cancelCheck();
  }

  async update(patch: OnlineSettingsPatch): Promise<OnlineView> {
    const next = { ...this.current };
    if (patch.phones !== null) next.phones = patch.phones;
    if (patch.sendSensitive !== null) next.sendSensitive = patch.sendSensitive;
    if (patch.limit !== null) {
      next.limit = clone(patch.limit);
      const reached =
        patch.limit.kind === "questions"
          ? next.questionsThisMonth >= patch.limit.questions
          : patch.limit.kind === "usd" && (next.spentThisMonthUsd ?? 0) >= (patch.limit.usd ?? 0);
      const state = next.state;
      if (state.type === "paused" && state.pause.type === "limit" && !reached) {
        next.state = { type: "on" };
      } else if (state.type === "on" && reached) {
        next.state = { type: "paused", pause: { type: "limit", until: nextMonth(this.now()) } };
      }
    }
    this.current = next;
    this.changed();
    return this.view();
  }

  async sends(limit: number, before: number | null): Promise<OnlineSend[]> {
    return this.log.filter((s) => before === null || s.id < before).slice(0, limit);
  }
}

/**
 * Where an address points, as the computer judges it: never a provider, this computer or the
 * person's own network (plain http too), or anywhere else (https only).
 */
function reach(hostname: string): "never" | "own" | "public" {
  const host = hostname.toLowerCase().replace(/\.$/, "");
  const never =
    /^(0\.|169\.254\.|2(2[4-9]|[3-5]\d)\.)/.test(host) ||
    /^\[(::|fe80:.*|ff.*)\]$/.test(host) ||
    /^(metadata|metadata\.google\.internal|instance-data)$/.test(host);
  if (never) return "never";
  const own =
    /^(127\.|10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.)/.test(
      host,
    ) ||
    /^\[(::1|f[cd][0-9a-f]*:.*)\]$/.test(host) ||
    /(^|\.)(localhost|local|home\.arpa|internal|ts\.net)$/.test(host) ||
    !host.includes(".");
  return own ? "own" : "public";
}
