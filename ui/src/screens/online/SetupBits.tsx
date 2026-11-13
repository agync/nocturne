import type { ReactNode } from "react";
import { t } from "../../copy/t";
import styles from "./Online.module.css";

/** "Online answers · step 3 of 5". */
export function StepLabel({ step }: { step: number }) {
  return <span className={styles.label}>{t("online.step", { step })}</span>;
}

/** The side column of a step: a small heading and a few plain lines. */
export function StepAside({
  heading,
  paragraphs,
  children,
}: {
  heading: string;
  paragraphs: string[];
  children?: ReactNode;
}) {
  return (
    <>
      <h2 className={styles.label}>{heading}</h2>
      {paragraphs.map((text, index) => (
        <p
          key={text}
          className={styles.sub}
          style={index > 0 ? { paddingTop: 12, borderTop: "1px solid var(--rule)" } : undefined}
        >
          {text}
        </p>
      ))}
      {children}
    </>
  );
}
