import type { ReactNode } from "react";
import styles from "./ScreenSheet.module.css";

interface ScreenSheetProps {
  children: ReactNode;
  /** No padding, for split views whose panes pad themselves. */
  flush?: boolean;
  className?: string;
}

export function ScreenSheet({ children, flush = false, className }: ScreenSheetProps) {
  return (
    <div className={styles.frame}>
      <div className={className ? `${styles.sheet} ${className}` : styles.sheet} data-flush={flush}>
        {children}
      </div>
    </div>
  );
}
