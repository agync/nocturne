import { t } from "../../copy/t";
import styles from "./Memories.module.css";
import { MemoryList } from "./MemoryList";
import { useMemories } from "./queries";
import { useForget } from "./useForget";

/** What matches a search of everything kept. */
export function Results({ query }: { query: string }) {
  const found = useMemories({ query });
  const forget = useForget();
  if (found.isError) {
    return (
      <p className={styles.message} role="alert">
        {t("memories.error")}
      </p>
    );
  }
  if (!found.data) {
    return (
      <p className={styles.message} role="status">
        {t("memories.loading")}
      </p>
    );
  }
  return (
    <div className={styles.entity}>
      <h2 className={`t-page-line ${styles.resultsHeading}`}>
        {t("memories.results.heading", { count: found.data.length, query })}
      </h2>
      <hr className={styles.rule} />
      {found.data.length === 0 ? (
        <p className={styles.empty}>{t("memories.empty.search", { query })}</p>
      ) : (
        <>
          <MemoryList memories={found.data} onForget={(m) => forget.open([m.id])} />
          <p className={styles.hint}>{t("memories.hint")}</p>
        </>
      )}
      {forget.dialog}
    </div>
  );
}
