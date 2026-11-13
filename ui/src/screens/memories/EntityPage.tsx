import type { ReactNode } from "react";
import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import { useEngine, useRefresh } from "../../engine/context";
import styles from "./Memories.module.css";
import { MemoryList } from "./MemoryList";
import { MEMORY_KEYS, useEntity, useMemories } from "./queries";
import { useForget } from "./useForget";

/** One person, place or thing, and what Nocturne remembers about them. */
export function EntityPage({ id }: { id: string }) {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const entity = useEntity(id);
  const memories = useMemories({ entityId: id });
  const heading = useRef<HTMLHeadingElement>(null);
  const [busy, setBusy] = useState(false);
  const forget = useForget(() => heading.current?.focus());

  const page = (node: ReactNode) => (
    <>
      {node}
      {forget.dialog}
    </>
  );

  if (entity.isError || memories.isError) {
    return page(
      <div className={styles.message} role="alert">
        <h1 className="t-page-title">{t("memories.entity.missing")}</h1>
        <a href="#/memories">{t("memories.back")}</a>
      </div>,
    );
  }
  if (!entity.data || !memories.data) {
    return page(
      <p className={styles.message} role="status">
        {t("memories.loading")}
      </p>,
    );
  }

  const e = entity.data;
  const rows = memories.data.filter((m) => m.status !== "letGo");
  const toMark = rows.filter((m) => !m.private);
  const summary = t("memories.entity.summary", { count: e.memoryCount, items: e.itemCount });

  const markAllPrivate = async () => {
    setBusy(true);
    try {
      for (const m of toMark) await engine.setPrivate(m.id, true);
      refresh(...MEMORY_KEYS);
      toast.show({ message: t("memories.markedPrivate", { count: toMark.length }) });
    } catch {
      refresh(...MEMORY_KEYS);
      toast.show({ message: t("memories.failed") });
    } finally {
      setBusy(false);
    }
  };

  return page(
    <div className={styles.entity}>
      <a className={styles.back} href="#/memories">
        {t("memories.back")}
      </a>
      <header className={styles.entityHead}>
        <div>
          <h1 ref={heading} tabIndex={-1} className={`t-page-title ${styles.name}`}>
            {e.name}
          </h1>
          <p className={styles.summary}>
            {e.role ? t("memories.entity.role", { role: e.role, summary }) : summary}
          </p>
        </div>
        <Button
          variant="quiet"
          disabled={busy || toMark.length === 0}
          onClick={() => void markAllPrivate()}
        >
          {toMark.length === 0 && rows.length > 0
            ? t("memories.allPrivate")
            : t("memories.markAllPrivate")}
        </Button>
      </header>
      <hr className={styles.rule} />
      {rows.length === 0 ? (
        <p className={styles.empty}>{t("memories.entity.none")}</p>
      ) : (
        <>
          <MemoryList memories={rows} onForget={(m) => forget.open([m.id])} />
          <p className={styles.hint}>{t("memories.hint")}</p>
        </>
      )}
    </div>,
  );
}
