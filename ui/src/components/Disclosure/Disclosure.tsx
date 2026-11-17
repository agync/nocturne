import { type ReactNode, useId, useState } from "react";
import { Chevron } from "./Chevron";
import styles from "./Disclosure.module.css";

/**
 * A line that shows more in place: the chevron points right while it is shut and down while
 * open. The words are not in the page until it opens, and the button names them only once open.
 */
export function Disclosure({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div className={styles.disclosure}>
      <button
        type="button"
        className={`${styles.toggle} hit`}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen(!open)}
      >
        <Chevron size={14} />
        {label}
      </button>
      {open ? (
        <div id={id} className={styles.panel}>
          {children}
        </div>
      ) : null}
    </div>
  );
}
