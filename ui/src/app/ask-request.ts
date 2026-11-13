import { useSyncExternalStore } from "react";

/**
 * Carries a question from the keep/ask bar to the Ask screen. The bar calls `request`, then
 * navigates to #/ask; the Ask screen reads `useAskRequest()` and calls `take()` once it starts.
 */
let pending: { id: number; question: string } | null = null;
let counter = 0;
const listeners = new Set<() => void>();

export const askRequests = {
  request(question: string): void {
    counter += 1;
    pending = { id: counter, question };
    for (const cb of listeners) cb();
  },
  peek(): { id: number; question: string } | null {
    return pending;
  },
  take(): string | null {
    const taken = pending?.question ?? null;
    pending = null;
    for (const cb of listeners) cb();
    return taken;
  },
  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
};

export function useAskRequest(): { id: number; question: string } | null {
  return useSyncExternalStore(askRequests.subscribe, askRequests.peek, askRequests.peek);
}
