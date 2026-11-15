import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import { npuFixable } from "./Advanced";
import styles from "./Settings.module.css";

const HIDDEN = "nocturne.fasterNote.hidden";

function readHidden(): boolean {
  try {
    return localStorage.getItem(HIDDEN) === "1";
  } catch {
    return false;
  }
}

function writeHidden(): void {
  try {
    localStorage.setItem(HIDDEN, "1");
  } catch {
    // Without storage the note stays hidden until the screen is opened again.
  }
}

/**
 * A quiet note, only when a part of this computer that would make Nocturne faster is not set up
 * and the person can set it up. Everything works without it, so it never sounds like a problem.
 */
export function FasterNote() {
  const engine = useEngine();
  const { data } = useQuery({ queryKey: ["advanced"], queryFn: () => engine.getAdvanced() });
  const [hidden, setHidden] = useState(readHidden);
  if (hidden || !npuFixable(data?.npu)) return null;
  return (
    <aside className={styles.faster}>
      <p className={styles.note}>{t("settings.faster.note")}</p>
      <div className={styles.fasterActions}>
        <a className="hit" href="#/settings/advanced">
          {t("settings.faster.how")}
        </a>
        <Button
          variant="quiet"
          onClick={() => {
            writeHidden();
            setHidden(true);
          }}
        >
          {t("settings.faster.hide")}
        </Button>
      </div>
    </aside>
  );
}
