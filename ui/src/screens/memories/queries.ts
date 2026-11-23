import { useQuery } from "@tanstack/react-query";
import type { EntityKind, MemoryFilter } from "../../engine/bindings";
import { useEngine } from "../../engine/context";

const none: MemoryFilter = {
  entityId: null,
  itemId: null,
  status: null,
  query: null,
  limit: null,
};

export function useEntities(kind: EntityKind | null, query: string | null) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["entities", kind, query ?? ""],
    queryFn: () => engine.listEntities(kind, query),
  });
}

export function useEntity(id: string | null) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["entities", "one", id],
    queryFn: () => engine.getEntity(id as string),
    enabled: id !== null,
  });
}

export function useEntityJournalDays(id: string, enabled: boolean) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["journal", "entity", id],
    queryFn: () => engine.entityJournalDays(id),
    enabled,
  });
}

export function useMemories(filter: Partial<MemoryFilter>, enabled = true) {
  const engine = useEngine();
  const full = { ...none, ...filter };
  return useQuery({
    queryKey: ["memories", full],
    queryFn: () => engine.listMemories(full),
    enabled,
  });
}

/** Kept things found by their words that no mind has read yet. */
export function useUnread(query: string) {
  const engine = useEngine();
  return useQuery({
    queryKey: ["memories", "unread", query],
    queryFn: () => engine.findUnread(query),
  });
}

export function useItem(id: string) {
  const engine = useEngine();
  return useQuery({ queryKey: ["item", id], queryFn: () => engine.getItem(id) });
}

/** What a change to a memory or item makes stale. */
export const MEMORY_KEYS = ["memories", "entities", "item", "fading", "page", "needsYou", "status"];
