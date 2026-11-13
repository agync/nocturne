import { useRef, useState } from "react";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import type { ForgetReport } from "../../engine/bindings";
import { useRefresh } from "../../engine/context";
import { MEMORY_KEYS } from "./queries";

/**
 * Holds the forget dialog above the rows it came from, so it can say "Gone." even after the
 * memory has left the list. The lists are re-read, and `onGone` runs, once the dialog closes.
 */
export function useForget(onGone?: (report: ForgetReport) => void, itemIds?: string[]) {
  const refresh = useRefresh();
  const [target, setTarget] = useState<string[] | null>(null);
  const done = useRef<ForgetReport | null>(null);

  const dialog = target ? (
    <ForgetDialog
      memoryIds={target}
      itemIds={itemIds}
      defaultIncludeOriginals
      onDone={(report) => {
        done.current = report;
      }}
      onClose={() => {
        setTarget(null);
        const report = done.current;
        done.current = null;
        if (report) {
          refresh(...MEMORY_KEYS, "items", "pages");
          // Once the dialog has gone and given focus back, so what `onGone` focuses is not inert.
          setTimeout(() => onGone?.(report), 0);
        }
      }}
    />
  ) : null;

  return { open: (ids: string[]) => setTarget(ids), dialog };
}
