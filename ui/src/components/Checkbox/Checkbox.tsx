import { useEffect, useId, useRef } from "react";
import styles from "./Checkbox.module.css";

interface CheckboxProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
  /** For a "select all" that stands for a mixed set. */
  indeterminate?: boolean;
}

/** A real checkbox with its words. The box edge is 3:1 against the page, and a tick shows state. */
export function Checkbox({
  checked,
  onChange,
  label,
  hint,
  disabled = false,
  indeterminate = false,
}: CheckboxProps) {
  const hintId = useId();
  const labelId = useId();
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (input.current) input.current.indeterminate = indeterminate;
  }, [indeterminate]);
  return (
    <label className={styles.row} data-disabled={disabled}>
      <input
        ref={input}
        type="checkbox"
        className={styles.input}
        checked={checked}
        disabled={disabled}
        aria-labelledby={labelId}
        aria-describedby={hint ? hintId : undefined}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span
        className={styles.box}
        aria-hidden="true"
        data-state={indeterminate ? "mixed" : checked ? "on" : "off"}
      />
      <span className={styles.text}>
        <span id={labelId} className={styles.label}>
          {label}
        </span>
        {hint ? (
          <span id={hintId} className={styles.hint}>
            {hint}
          </span>
        ) : null}
      </span>
    </label>
  );
}
