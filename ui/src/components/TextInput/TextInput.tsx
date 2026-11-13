import { type InputHTMLAttributes, type Ref, useId } from "react";
import styles from "./TextInput.module.css";

interface TextInputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "className" | "onChange" | "value" | "id"> {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  /** What is wrong, in a sentence. Its presence marks the field invalid. */
  error?: string;
  inputRef?: Ref<HTMLInputElement>;
}

/** A labelled field. The label is always visible; the hint and the error are read with it. */
export function TextInput({
  label,
  value,
  onChange,
  hint,
  error,
  inputRef,
  ...rest
}: TextInputProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ");
  return (
    <div className={styles.field} data-invalid={Boolean(error)}>
      <label htmlFor={id} className={styles.label}>
        {label}
      </label>
      {hint ? (
        <span id={hintId} className={styles.hint}>
          {hint}
        </span>
      ) : null}
      <input
        {...rest}
        ref={inputRef}
        id={id}
        className={styles.input}
        value={value}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy || undefined}
        onChange={(event) => onChange(event.target.value)}
      />
      <span id={errorId} className={styles.error} role="alert">
        {error ?? ""}
      </span>
    </div>
  );
}
