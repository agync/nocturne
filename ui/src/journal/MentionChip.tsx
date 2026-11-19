import type { ButtonHTMLAttributes } from "react";
import { t } from "../copy/t";
import type { EntityKind } from "../engine/bindings";
import styles from "./MentionCard.module.css";
import { NameIcon } from "./NameIcon";

interface MentionChipProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "className" | "children"> {
  name: string;
  kind: EntityKind;
  /** A name Nocturne has not seen before carries "new". */
  isNew: boolean;
  /** The chip whose card is open. */
  pressed?: boolean;
}

/** A person, place or thing named in the journal. Pressing it opens its card. */
export function MentionChip({ name, kind, isNew, pressed = false, ...rest }: MentionChipProps) {
  return (
    <button
      type="button"
      className={styles.chip}
      aria-haspopup="dialog"
      aria-expanded={pressed}
      {...rest}
    >
      <NameIcon kind={kind} />
      <span>{name}</span>
      {isNew ? <span className={styles.newTag}>{t("journal.people.new")}</span> : null}
    </button>
  );
}
