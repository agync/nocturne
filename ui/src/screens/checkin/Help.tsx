import { useId, useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import { useEngine } from "../../engine/context";
import styles from "./Checkin.module.css";

/** Ideas are kept by their short id, "slow.name", which the recap words again. */
const IDEA = "journal.help.idea.";

const KINDS = [
  {
    name: "journal.help.kind.move",
    ideas: ["journal.help.idea.move.walk", "journal.help.idea.move.shake"],
  },
  {
    name: "journal.help.kind.slow",
    ideas: ["journal.help.idea.slow.breathe", "journal.help.idea.slow.name"],
  },
  {
    name: "journal.help.kind.think",
    ideas: ["journal.help.idea.think.write", "journal.help.idea.think.week"],
  },
  {
    name: "journal.help.kind.reach",
    ideas: ["journal.help.idea.reach.message", "journal.help.idea.reach.tell"],
  },
] as const;

type Idea = (typeof KINDS)[number]["ideas"][number];

/** Four kinds of idea, one chosen at a time, and the safety line. Never says whether it worked. */
export function Help({ checkinId }: { checkinId: string }) {
  const engine = useEngine();
  const toast = useToast();
  const group = useId();
  const noteId = useId();
  const [idea, setIdea] = useState<Idea | null>(null);
  const [noted, setNoted] = useState(false);
  const [working, setWorking] = useState(false);

  const tried = async () => {
    if (!idea || working) return;
    setWorking(true);
    try {
      await engine.markTried(checkinId, idea.slice(IDEA.length));
      setNoted(true);
    } catch {
      toast.show({ message: t("notice.action.failed") });
    } finally {
      setWorking(false);
    }
  };

  return (
    <section className={styles.help} aria-labelledby={`${group}-title`}>
      <div>
        <h2 id={`${group}-title`} className={styles.helpTitle}>
          {t("journal.help.title")}
        </h2>
        <p className={styles.muted}>{t("journal.help.lead")}</p>
      </div>
      <div className={styles.helpKinds}>
        {KINDS.map((kind) => (
          <fieldset key={kind.name} className={styles.helpKind}>
            <legend>{t(kind.name)}</legend>
            {kind.ideas.map((id) => (
              <label key={id} className={styles.idea}>
                <input
                  type="radio"
                  name={group}
                  className={styles.ideaInput}
                  checked={idea === id}
                  onChange={() => {
                    setIdea(id);
                    setNoted(false);
                  }}
                />
                <span className={styles.radio} aria-hidden="true" />
                {t(id)}
              </label>
            ))}
          </fieldset>
        ))}
      </div>
      <div className={styles.buttons}>
        <Button
          variant="secondary"
          aria-disabled={!idea || undefined}
          aria-describedby={noteId}
          onClick={() => void tried()}
        >
          {t("journal.help.tried")}
        </Button>
        <span id={noteId} className={styles.small} role="status">
          {noted ? t("journal.help.triedDone") : idea ? "" : t("journal.help.triedNone")}
        </span>
      </div>
      <p className={styles.safety}>{t("journal.help.safety")}</p>
    </section>
  );
}
