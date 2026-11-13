import type { ButtonHTMLAttributes, ReactNode } from "react";
import styles from "./Button.module.css";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "forget";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className"> {
  variant?: ButtonVariant;
  /** 44 px target for touch. The default is 40 px. */
  touch?: boolean;
  /** Quiet only: pulls the label back so its text lines up with the content edge. */
  flush?: boolean;
  /** Forget only: the confirm step, the one place it may be filled. */
  confirming?: boolean;
  children: ReactNode;
}

export function Button({
  variant = "secondary",
  touch = false,
  flush = false,
  confirming = false,
  type = "button",
  children,
  ...rest
}: ButtonProps) {
  // Forget is outlined until the person reaches the confirm step.
  const filled = variant === "primary" || (variant === "forget" && confirming);
  return (
    <button
      type={type}
      className={styles.button}
      data-variant={variant}
      data-state={confirming ? "confirming" : "rest"}
      data-filled={filled}
      data-touch={touch}
      data-flush={flush}
      {...rest}
    >
      {children}
    </button>
  );
}
