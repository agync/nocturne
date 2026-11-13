import { useState } from "react";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { MemoryView } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { MEMORY_KEYS } from "./queries";
import { RefileDialog } from "./RefileDialog";

/**
 * Holds the "Is this about something else?" dialog above the rows it came from, so it outlives
 * a memory that has just left the list (or been replaced by a correction). Moving says where it
 * went and offers to undo it.
 */
export function useRefile() {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const [target, setTarget] = useState<MemoryView | null>(null);

  const dialog = target ? (
    <RefileDialog
      memory={target}
      onClose={() => setTarget(null)}
      onDone={(moved) => {
        setTarget(null);
        refresh(...MEMORY_KEYS);
        toast.show({
          message: t("refile.done", { name: moved.entity.name }),
          action: {
            label: t("memories.undo"),
            onAction: () => {
              engine.undoChange(moved.changeId).then(
                () => {
                  refresh(...MEMORY_KEYS);
                  toast.show({ message: t("refile.undone") });
                },
                () => toast.show({ message: t("memories.failed") }),
              );
            },
          },
        });
      }}
    />
  ) : null;

  return { open: (memory: MemoryView) => setTarget(memory), dialog };
}
