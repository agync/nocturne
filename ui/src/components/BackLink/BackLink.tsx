import styles from "./BackLink.module.css";

interface BackLinkProps {
  /** Route path of the destination, such as "/settings". */
  to: string;
  label: string;
}

export function BackLink({ to, label }: BackLinkProps) {
  return (
    <a className={`${styles.back} hit`} href={`#${to}`}>
      <span aria-hidden="true">←</span>
      {label}
    </a>
  );
}
