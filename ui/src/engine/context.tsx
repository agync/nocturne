import {
  QueryClient,
  QueryClientProvider,
  type QueryKey,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  AskDelta,
  AskPhase,
  AskPlace,
  ItemFilter,
  NearMatch,
  Period,
  PeriodKind,
  Receipt,
} from "./bindings";
import type { EngineClient, EngineEventName, EngineEvents } from "./client";

/** Which cached queries an engine event makes stale. */
export const EVENT_INVALIDATIONS: Readonly<Partial<Record<EngineEventName, QueryKey[]>>> = {
  "status-changed": [["status"], ["journal"]],
  "item-changed": [["items"], ["page"], ["needsYou"], ["journal"]],
  "page-ready": [["page"], ["pages"]],
  "needs-you-changed": [["needsYou"], ["status"]],
  "setup-progress": [["setup"]],
  /** Progress only: the screen that started the export listens for it directly. */
  "export-progress": [],
};

const EngineContext = createContext<EngineClient | null>(null);

const READY_FALLBACK_MS = 150;

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { staleTime: 60_000, retry: false, refetchOnWindowFocus: false },
    },
  });
}

interface ProviderProps {
  engine: EngineClient;
  queryClient?: QueryClient;
  children: ReactNode;
}

export function EngineProvider({ engine, queryClient, children }: ProviderProps) {
  const client = useMemo(() => queryClient ?? createQueryClient(), [queryClient]);

  useEffect(() => {
    const offs = (Object.keys(EVENT_INVALIDATIONS) as EngineEventName[]).map((name) =>
      engine.on(name, () => {
        for (const queryKey of EVENT_INVALIDATIONS[name] ?? []) {
          void client.invalidateQueries({ queryKey });
        }
      }),
    );
    return () => {
      for (const off of offs) off();
    };
  }, [engine, client]);

  // The window stays hidden until the first screen has painted; then the shell shows it. A hidden
  // web view may never run animation frames, so a short timer stands in for them.
  useEffect(() => {
    let frame = 0;
    let sent = false;
    const ready = () => {
      if (sent) return;
      sent = true;
      void engine.appReady().catch((error: unknown) => console.error("app_ready failed", error));
    };
    const timer = setTimeout(ready, READY_FALLBACK_MS);
    if (typeof requestAnimationFrame === "function") {
      frame = requestAnimationFrame(() => {
        frame = requestAnimationFrame(ready);
      });
    }
    return () => {
      clearTimeout(timer);
      if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(frame);
    };
  }, [engine]);

  return (
    <QueryClientProvider client={client}>
      <EngineContext.Provider value={engine}>{children}</EngineContext.Provider>
    </QueryClientProvider>
  );
}

export function useEngine(): EngineClient {
  const engine = useContext(EngineContext);
  if (!engine) throw new Error("useEngine needs an EngineProvider");
  return engine;
}

export function useStatus() {
  const engine = useEngine();
  return useQuery({ queryKey: ["status"], queryFn: () => engine.getStatus() });
}

export function usePage(date?: string) {
  const engine = useEngine();
  return useQuery({ queryKey: ["page", date ?? "today"], queryFn: () => engine.getPage(date) });
}

export function usePages() {
  const engine = useEngine();
  return useQuery({ queryKey: ["pages"], queryFn: () => engine.listPages() });
}

export function useItems(filter: ItemFilter, enabled = true) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["items", filter],
    queryFn: () => engine.listItems(filter),
    enabled,
  });
}

export function useNeedsYou() {
  const engine = useEngine();
  return useQuery({ queryKey: ["needsYou"], queryFn: () => engine.listNeedsYou() });
}

export function useSettings() {
  const engine = useEngine();
  return useQuery({ queryKey: ["settings"], queryFn: () => engine.getSettings() });
}

/** One journal day; today when `day` is absent. */
export function useJournalDay(day?: string) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["journal", "day", day ?? "today"],
    queryFn: () => engine.getJournalDay(day),
  });
}

/** Every day with anything in the journal, oldest first. */
export function useJournalDays() {
  const engine = useEngine();
  return useQuery({ queryKey: ["journal", "days"], queryFn: () => engine.listJournalDays() });
}

/** Weeks, months or years with anything kept, newest first. */
export function usePeriods(kind: PeriodKind) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["journal", "periods", kind],
    queryFn: () => engine.listPeriods(kind),
  });
}

/** The reflection or recap for a period, or nothing while no period is chosen. */
export function useReflection(period: Period | null) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["journal", "reflection", period?.kind, period?.start, period?.end],
    queryFn: () => {
      if (!period) throw new Error("No period");
      return engine.getReflection(period);
    },
    enabled: period !== null,
    // From Health and what went together are worked out from what is kept now, which a sync, a
    // removal in Health or the noticing switch changes without a word.
    staleTime: 0,
  });
}

/** Re-read the named queries after a change the engine does not announce itself. */
export function useRefresh() {
  const client = useQueryClient();
  return useCallback(
    (...keys: string[]) => {
      for (const key of keys) void client.invalidateQueries({ queryKey: [key] });
    },
    [client],
  );
}

export function useEngineEvent<E extends EngineEventName>(
  name: E,
  cb: (payload: EngineEvents[E]) => void,
): void {
  const engine = useEngine();
  const latest = useRef(cb);
  latest.current = cb;
  useEffect(() => engine.on(name, (payload) => latest.current(payload)), [engine, name]);
}

export type AskState = "idle" | "asking" | AskPhase;

export interface AskResult {
  /** Resolves with the ask id, or null when the engine refused or a newer question took over. */
  ask: (question: string, place?: AskPlace, help?: boolean) => Promise<string | null>;
  /** Set when the engine could not take the question; the screen says so in words. */
  error: Error | null;
  askId: string | null;
  deltas: AskDelta[];
  phase: AskState;
  text: string;
  receipts: Receipt[];
  near: NearMatch[];
  found: number | null;
}

const MAX_EARLY = 200;

const TERMINAL: ReadonlySet<AskPhase> = new Set(["done", "notSure", "nothing", "unavailable"]);

const receiptKey = (r: Receipt) => `${r.itemId}:${JSON.stringify(r.span)}`;

/**
 * Starts a question and follows its answer. Deltas that arrive before `ask()` has returned its id
 * are kept and replayed, so a fast engine never loses the start of an answer.
 */
export function useAsk(): AskResult {
  const engine = useEngine();
  const [askId, setAskId] = useState<string | null>(null);
  const [deltas, setDeltas] = useState<AskDelta[]>([]);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const active = useRef<string | null>(null);
  const early = useRef<AskDelta[]>([]);
  const call = useRef(0);

  useEffect(
    () =>
      engine.on("ask-delta", (delta) => {
        if (active.current === null) {
          // Deltas before the id is known are kept, but never without limit.
          if (early.current.length < MAX_EARLY) early.current.push(delta);
        } else if (delta.askId === active.current) {
          setDeltas((previous) => [...previous, delta]);
        }
      }),
    [engine],
  );

  const ask = useCallback(
    async (question: string, place?: AskPlace, help?: boolean) => {
      const mine = ++call.current;
      // One question at a time: the one on screen is stopped before the next starts.
      const previous = active.current;
      if (previous !== null) void engine.cancelAsk(previous).catch(() => undefined);
      active.current = null;
      early.current = [];
      setAsking(true);
      setError(null);
      setDeltas([]);
      setAskId(null);
      let id: string;
      try {
        id = help ? await engine.ask(question, place, true) : await engine.ask(question, place);
      } catch (failure) {
        if (mine === call.current) {
          setAsking(false);
          setError(failure instanceof Error ? failure : new Error(String(failure)));
        }
        return null;
      }
      // A newer question has started since: this answer is no longer the one on screen.
      if (mine !== call.current) return null;
      active.current = id;
      setAskId(id);
      setDeltas(early.current.filter((d) => d.askId === id));
      early.current = [];
      return id;
    },
    [engine],
  );

  const last = deltas.at(-1);
  const phase: AskState = last ? last.phase : asking ? "asking" : "idle";
  const receipts: Receipt[] = [];
  const seen = new Set<string>();
  for (const d of deltas) {
    for (const r of d.receipts) {
      const key = receiptKey(r);
      if (!seen.has(key)) {
        seen.add(key);
        receipts.push(r);
      }
    }
  }
  const streamed = deltas.map((d) => d.textDelta ?? "").join("");
  const text = deltas.some((d) => d.plain) ? streamed.replaceAll("**", "") : streamed;
  const near = deltas.flatMap((d) => d.near);
  const found = [...deltas].reverse().find((d) => d.found !== null)?.found ?? null;

  useEffect(() => {
    if (last && TERMINAL.has(last.phase)) setAsking(false);
  }, [last]);

  return { ask, error, askId, deltas, phase, text, receipts, near, found };
}
