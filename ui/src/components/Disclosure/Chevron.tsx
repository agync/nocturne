import { Icon } from "../Icon/Icon";
import styles from "./Chevron.module.css";

/**
 * The chevron before the words of a button that shows or hides text in place: right while shut,
 * down while open. Put it first inside a button that carries `aria-expanded`.
 */
export function Chevron({ size = 16 }: { size?: number }) {
  return (
    <span className={styles.chevron} data-chevron="">
      <Icon name="chevron-down" size={size} />
    </span>
  );
}
