import { useEffect, useState } from "react";
import { t } from "../../copy/t";
import { usePlatform } from "../../platform/context";
import styles from "./DropOverlay.module.css";

interface DropOverlayProps {
  onDrop: (paths: string[]) => void;
}

/** Drop anywhere: the whole window says what will happen, and nothing else does. */
export function DropOverlay({ onDrop }: DropOverlayProps) {
  const platform = usePlatform();
  const [count, setCount] = useState<number | null>(null);

  useEffect(() => platform.onFileDrop(onDrop, setCount), [platform, onDrop]);

  if (count === null || count < 1) return null;
  return (
    <output className={styles.overlay} aria-live="polite">
      <span className={styles.card}>
        <span className={styles.title}>{t("shell.drop.title", { count })}</span>
        <span className={styles.body}>{t("shell.drop.body")}</span>
      </span>
    </output>
  );
}
