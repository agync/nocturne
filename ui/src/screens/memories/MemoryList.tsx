import { useState } from "react";
import type { MemoryView } from "../../engine/bindings";
import styles from "./Memories.module.css";
import { MemoryRow } from "./MemoryRow";

interface MemoryListProps {
  memories: MemoryView[];
  onForget: (memory: MemoryView) => void;
  onRefile: (memory: MemoryView) => void;
  label?: string;
}

/** Memory rows with one of them selected at a time. */
export function MemoryList({ memories, onForget, onRefile, label }: MemoryListProps) {
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <ul className={styles.memories} aria-label={label}>
      {memories.map((m) => (
        <MemoryRow
          key={m.id}
          memory={m}
          selected={selected === m.id}
          onSelect={setSelected}
          onForget={onForget}
          onRefile={onRefile}
        />
      ))}
    </ul>
  );
}
