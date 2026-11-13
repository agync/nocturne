import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ModelsView } from "../../engine/bindings";
import { useEngine, useEngineEvent } from "../../engine/context";

export const MODELS_KEY = ["models"] as const;

/**
 * Not read yet: until choosing models is built, the view stays `undefined`, so its Settings row
 * and links stay hidden. Tests turn it on to see them.
 */
export const modelsReads = { on: false };

/** The model for each job as the computer last said it, following `models-changed`. */
export function useModels(): { data?: ModelsView; refetch(): void } {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: MODELS_KEY,
    queryFn: () => engine.getModels(),
    enabled: modelsReads.on,
    staleTime: 5_000,
  });
  useEngineEvent("models-changed", (view) => {
    if (modelsReads.on) client.setQueryData(MODELS_KEY, view);
  });
  return { data: query.data, refetch: () => void query.refetch() };
}
