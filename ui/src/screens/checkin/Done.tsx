import { useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { CheckIn } from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import { PARTS } from "../../journal/feelings";
import styles from "./Checkin.module.css";
import { Help } from "./Help";
import { sentence } from "./words";

interface DoneProps {
  checkin: CheckIn;
  onChange: () => void;
}

/** "Saved. Relieved, content." with a way back, a way to change it, and for heavy ones some ideas. */
export function Done({ checkin, onChange }: DoneProps) {
  const title = useRef<HTMLHeadingElement>(null);
  const [helping, setHelping] = useState(false);
  const { part, words } = checkin.feeling;
  const name = t(PARTS[part].name);
  const heavy = part === "woundUp" || part === "low";

  useEffect(() => title.current?.focus({ preventScroll: true }), []);

  return (
    <div className={styles.done}>
      <div className="sr-only" role="status">
        {t("journal.a11y.saved")}
      </div>
      <h1 ref={title} tabIndex={-1} className="t-page-title">
        {words.length > 0
          ? t("journal.checkin.saved", { words: sentence(words) })
          : t("journal.checkin.savedBare")}
      </h1>
      <p className={styles.muted}>
        <span className={styles.feel} data-part={part}>
          <FeelingMark mark={part} size={14} />
          {words.length > 0
            ? t("journal.checkin.feel", { part: name, words: words.join(", ") })
            : name}
        </span>{" "}
        {t("journal.checkin.doneAt", { time: formatTime(checkin.keptAt) })}
      </p>
      {heavy ? (
        <Button variant="quiet" flush aria-expanded={helping} onClick={() => setHelping(!helping)}>
          {t("journal.checkin.helpLink")}
        </Button>
      ) : null}
      {heavy && helping ? <Help checkinId={checkin.id} /> : null}
      <div className={styles.buttons}>
        <Button variant="secondary" onClick={() => goTo("/journal")}>
          {t("journal.checkin.back")}
        </Button>
        <Button variant="quiet" onClick={onChange}>
          {t("journal.timeline.change")}
        </Button>
      </div>
    </div>
  );
}
