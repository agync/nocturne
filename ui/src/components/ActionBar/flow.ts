import { useSyncExternalStore } from "react";

let open = 0;
const listeners = new Set<() => void>();

const emit = () => {
  for (const listener of listeners) listener();
};

/** A step flow is on screen while its action bar is. Returns the call that says it has left. */
export function enterFlow(): () => void {
  open += 1;
  emit();
  return () => {
    open -= 1;
    emit();
  };
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

/** True while any step flow is open, so the shell can give its bottom edge to the action bar. */
export function useFlowOpen(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => open > 0,
    () => false,
  );
}
