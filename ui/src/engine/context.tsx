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
import type { AskDelta, AskPhase, ItemFilter, NearMatch, Receipt } from "./bindings";
import type { EngineClient, EngineEventName, EngineEvents } from "./client";

/** Which cached queries an engine event makes stale. */
export const EVENT_INVALIDATIONS: Readonly<Partial<Record<EngineEventName, QueryKey[]>>> = {
  "status-changed": [["status"]],
  "item-changed": [["items"], ["page"], ["needsYou"]],
  "page-ready": [["page"], ["pages"]],
  "needs-you-changed": [["needsYou"], ["status"]],
  "setup-progress": [["setup"]],
};

const EngineContext = createContext<EngineClient | null>(null);

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
  ask: (question: string) => Promise<string>;
  askId: string | null;
  deltas: AskDelta[];
  phase: AskState;
  text: string;
  receipts: Receipt[];
  near: NearMatch[];
  found: number | null;
}

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
  const active = useRef<string | null>(null);
  const early = useRef<AskDelta[]>([]);

  useEffect(
    () =>
      engine.on("ask-delta", (delta) => {
        if (active.current === null) {
          early.current.push(delta);
        } else if (delta.askId === active.current) {
          setDeltas((previous) => [...previous, delta]);
        }
      }),
    [engine],
  );

  const ask = useCallback(
    async (question: string) => {
      active.current = null;
      early.current = [];
      setAsking(true);
      setDeltas([]);
      setAskId(null);
      const id = await engine.ask(question);
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
  const text = deltas.map((d) => d.textDelta ?? "").join("");
  const near = deltas.flatMap((d) => d.near);
  const found = [...deltas].reverse().find((d) => d.found !== null)?.found ?? null;

  useEffect(() => {
    if (last && TERMINAL.has(last.phase)) setAsking(false);
  }, [last]);

  return { ask, askId, deltas, phase, text, receipts, near, found };
}
