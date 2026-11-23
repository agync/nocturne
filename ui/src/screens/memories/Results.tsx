import { goTo } from "../../app/query";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { receiptPath } from "../../components/ReceiptChip/receiptPath";
import { t } from "../../copy/t";
import type { FoundItem } from "../../engine/bindings";
import { useStatus } from "../../engine/context";
import styles from "./Memories.module.css";
import { MemoryList } from "./MemoryList";
import { useMemories, useUnread } from "./queries";
import { useForget } from "./useForget";
import { useRefile } from "./useRefile";

/** What matches a search of everything kept. */
export function Results({ query }: { query: string }) {
  const found = useMemories({ query });
  // What no mind has read yet is found by its words meanwhile.
  const unread = useUnread(query).data ?? [];
  const forget = useForget();
  const refile = useRefile();
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
      {found.data.length === 0 && unread.length === 0 ? (
        <p className={styles.empty}>{t("memories.empty.search", { query })}</p>
      ) : found.data.length === 0 ? null : (
        <>
          <MemoryList
            memories={found.data}
            onForget={(m) => forget.open([m.id])}
            onRefile={refile.open}
          />
          <p className={styles.hint}>{t("memories.hint")}</p>
        </>
      )}
      {unread.length > 0 ? <Unread found={unread} /> : null}
      {forget.dialog}
      {refile.dialog}
    </div>
  );
}

/**
 * Kept things that match by their words and wait to be read, each with its original. With no
 * mind yet, the line says answers need one; with one, that they are read tonight.
 */
function Unread({ found }: { found: FoundItem[] }) {
  const mindless = useStatus().data?.state === "gettingReady";
  const lead = t(mindless ? "phone.alone.find.above" : "item.status.keptTonight");
  return (
    <section aria-label={lead}>
      <p className={styles.hint}>{lead}</p>
      <ul className={styles.unread}>
        {found.map((f) => (
          <li key={f.receipt.itemId}>
            <p>
              {f.excerpt.map((part, n) =>
                part.matched ? (
                  // biome-ignore lint/suspicious/noArrayIndexKey: parts of one excerpt keep their order
                  <mark key={n}>{part.text}</mark>
                ) : (
                  // biome-ignore lint/suspicious/noArrayIndexKey: parts of one excerpt keep their order
                  <span key={n}>{part.text}</span>
                ),
              )}
            </p>
            <ReceiptChip receipt={f.receipt} onOpen={() => goTo(receiptPath(f.receipt))} />
          </li>
        ))}
      </ul>
    </section>
  );
}
