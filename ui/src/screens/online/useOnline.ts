import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { OnlineView } from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine, useEngineEvent } from "../../engine/context";

export const ONLINE_KEY = ["online"] as const;

/** Online answers as the computer last said it, following `online-changed`. */
export function useOnline(): { data?: OnlineView; isError: boolean; refetch(): void } {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ONLINE_KEY,
    queryFn: () => engine.getOnline(),
    staleTime: 5_000,
  });
  useEngineEvent("online-changed", (view) => {
    client.setQueryData(ONLINE_KEY, view);
    void client.invalidateQueries({ queryKey: ["privacy"] });
    void client.invalidateQueries({ queryKey: ["advanced"] });
  });
  return { data: query.data, isError: query.isError, refetch: () => void query.refetch() };
}

/**
 * Runs something the person asked for, then re-reads the computer: a newer `online-changed`
 * may have arrived first, and a reply must never overwrite it. A refusal becomes one plain
 * sentence and the screen stays as it was. Resolves to the call's result, or `undefined`.
 */
export function useOnlineAct() {
  const client = useQueryClient();
  const toast = useToast();
  const flying = useRef(false);
  const [busy, setBusy] = useState(false);
  const run = useCallback(
    async <T>(
      call: () => Promise<T>,
      options: { quiet?: boolean; onError?: (code: string) => void } = {},
    ): Promise<{ value: T } | null> => {
      if (flying.current) return null;
      flying.current = true;
      setBusy(true);
      try {
        const value = await call();
        void client.invalidateQueries({ queryKey: ONLINE_KEY });
        void client.invalidateQueries({ queryKey: ["privacy"] });
        void client.invalidateQueries({ queryKey: ["advanced"] });
        return { value };
      } catch (error) {
        options.onError?.(toEngineError(error).code);
        if (!options.quiet) toast.show({ message: t("online.failed") });
        return null;
      } finally {
        flying.current = false;
        setBusy(false);
      }
    },
    [client, toast],
  );
  return { run, busy };
}
