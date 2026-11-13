import { useId } from "react";
import { t } from "../../copy/t";
import styles from "./Toggle.module.css";

interface ToggleProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}

/** A switch with its words. State is the knob and the word On or Off, never colour alone. */
export function Toggle({ checked, onChange, label, hint, disabled = false }: ToggleProps) {
  const labelId = useId();
  const hintId = useId();
  return (
    <label className={styles.row} data-disabled={disabled}>
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
      <span className={styles.control}>
        <span className={styles.state} aria-hidden="true">
          {checked ? t("toggle.on") : t("toggle.off")}
        </span>
        <button
          type="button"
          role="switch"
          aria-checked={checked}
          aria-labelledby={labelId}
          aria-describedby={hint ? hintId : undefined}
          className={styles.switch}
          data-state={checked ? "on" : "off"}
          disabled={disabled}
          onClick={() => onChange(!checked)}
        >
          <span className={styles.knob} />
        </button>
      </span>
    </label>
  );
}
