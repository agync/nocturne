import { t } from "../../copy/t";
import type { ItemStatus } from "../../engine/bindings";
import styles from "./StatusLabel.module.css";

export type LabelStatus = ItemStatus | "fading" | "letGo" | "private" | "keepForever";

interface StatusLabelProps {
  status: LabelStatus;
  /** For kept items: say when they will be read. */
  tonight?: boolean;
}

const WORDS: Record<LabelStatus, Parameters<typeof t>[0]> = {
  kept: "item.status.kept",
  reading: "item.status.reading",
  understood: "item.status.understood",
  partlyUnderstood: "item.status.partlyUnderstood",
  couldntRead: "item.status.couldntRead",
  needsYou: "item.status.needsYou",
  unconfirmed: "item.status.unconfirmed",
  fading: "memory.status.fading",
  letGo: "memory.status.letGo",
  private: "memory.flag.private",
  keepForever: "memory.flag.keepForever",
};

/** Words, not badges. Colour never carries the meaning alone. */
export function StatusLabel({ status, tonight = false }: StatusLabelProps) {
  const id = status === "kept" && tonight ? "item.status.keptTonight" : WORDS[status];
  return (
    <span className={styles.label} data-status={status}>
      {(status === "kept" || status === "reading") && (
        <span className={styles.dot} aria-hidden="true" />
      )}
      {(status === "understood" || status === "partlyUnderstood") && (
        <span className={styles.check} aria-hidden="true" />
      )}
      {t(id)}
    </span>
  );
}
