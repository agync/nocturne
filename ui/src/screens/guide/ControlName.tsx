import type { ReactNode } from "react";
import styles from "./Guide.module.css";

/**
 * A screen or control of Nocturne named in the guide, in its own words. Plain bold text for now:
 * this is the one place the control chip (`docs/design/control-chips.md`) replaces once it is
 * built.
 */
export function ControlName({ children }: { children: ReactNode }) {
  return <strong className={styles.control}>{children}</strong>;
}
