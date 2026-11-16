import { t } from "../../copy/t";
import { Icon } from "../Icon/Icon";
import styles from "./ControlChip.module.css";
import { type ChipId, chipSpec } from "./registry";

/**
 * A place or control in Nocturne, named inside a sentence with its own icon and words. It is a
 * plain span: never clickable, never focusable, and read out as the words it holds.
 */
export function ControlChip({ id }: { id: ChipId }) {
  const { copy, icon } = chipSpec(id);
  return (
    <span className={styles.chip} data-chip={id}>
      {icon ? <Icon name={icon} size={14} /> : null}
      {t(copy)}
    </span>
  );
}
