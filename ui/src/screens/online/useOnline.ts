import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { OnlineView } from "../../engine/bindings";
import { useEngine, useEngineEvent } from "../../engine/context";

export const ONLINE_KEY = ["online"] as const;

/**
 * Not read yet: until Online answers is built, the view stays `undefined`, so its Settings row
 * and links stay hidden. Tests turn it on to see them.
 */
export const onlineReads = { on: false };

/** Online answers as the computer last said it, following `online-changed`. */
export function useOnline(): { data?: OnlineView; refetch(): void } {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ONLINE_KEY,
    queryFn: () => engine.getOnline(),
    enabled: onlineReads.on,
    staleTime: 5_000,
  });
  useEngineEvent("online-changed", (view) => {
    if (onlineReads.on) client.setQueryData(ONLINE_KEY, view);
  });
  return { data: query.data, refetch: () => void query.refetch() };
}
