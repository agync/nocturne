import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { RemoteSettingsPatch } from "../../engine/bindings";
import { useEngine, useEngineEvent } from "../../engine/context";

export const REMOTE_KEY = ["remote"] as const;

/**
 * Away from home as the computer last said it. It reads `getRemote` once and then follows
 * `remote-changed`, and re-reads on `pairing-request` and `device-active`, which carry only part of it.
 */
export function useRemote() {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: REMOTE_KEY,
    queryFn: () => engine.getRemote(),
    staleTime: 5_000,
  });
  const reread = () => void client.invalidateQueries({ queryKey: REMOTE_KEY });
  useEngineEvent("remote-changed", (view) => {
    client.setQueryData(REMOTE_KEY, view);
    void client.invalidateQueries({ queryKey: ["privacy"] });
    void client.invalidateQueries({ queryKey: ["advanced"] });
  });
  useEngineEvent("pairing-request", reread);
  useEngineEvent("device-active", reread);
  return query;
}

/**
 * Runs something the person asked for. Whatever the reply says, the screen then re-reads the
 * computer: a newer `remote-changed` may have arrived first, and a reply must never overwrite it.
 * A refusal becomes one plain sentence, and the screen stays as it was. True when it worked.
 */
export function useRemoteAct() {
  const client = useQueryClient();
  const toast = useToast();
  return useCallback(
    async (call: () => Promise<unknown>): Promise<boolean> => {
      try {
        await call();
        void client.invalidateQueries({ queryKey: REMOTE_KEY });
        void client.invalidateQueries({ queryKey: ["advanced"] });
        return true;
      } catch {
        toast.show({ message: t("away.failed") });
        return false;
      }
    },
    [client, toast],
  );
}

/** Like `useRemoteAct`, but a second press while the first is still going does nothing, and `busy` says so. */
export function useBusyAct() {
  const act = useRemoteAct();
  const flying = useRef(false);
  const [busy, setBusy] = useState(false);
  const run = useCallback(
    async (call: () => Promise<unknown>): Promise<boolean> => {
      if (flying.current) return false;
      flying.current = true;
      setBusy(true);
      try {
        return await act(call);
      } finally {
        flying.current = false;
        setBusy(false);
      }
    },
    [act],
  );
  return { run, busy };
}

/** A settings patch that changes only what it names. */
export const settingsPatch = (fields: Partial<RemoteSettingsPatch>): RemoteSettingsPatch => ({
  connection: null,
  autoOff: null,
  keepAwake: null,
  ...fields,
});
