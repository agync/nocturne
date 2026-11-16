import type { ReactNode } from "react";
import styles from "./KeyCap.module.css";

/** A keyboard key named inside a sentence. Like a control chip, it is text and does nothing. */
export function KeyCap({ children }: { children: ReactNode }) {
  return <kbd className={styles.cap}>{children}</kbd>;
}

/** Several keys pressed together, one cap each, read as the words with a space between. */
export function KeyCaps({ keys }: { keys: string[] }) {
  return (
    <>
      {keys.map((key, index) => (
        <span key={key}>
          {index > 0 ? " " : null}
          <KeyCap>{key}</KeyCap>
        </span>
      ))}
    </>
  );
}
