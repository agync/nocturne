import type { ReactNode } from "react";
import { useEffect, useId, useRef, useState } from "react";
import { useLocation } from "wouter";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { formatDay } from "../../copy/format";
import { t } from "../../copy/t";
import { useEngine, useRefresh } from "../../engine/context";
import styles from "./Memories.module.css";
import { MemoryList } from "./MemoryList";
import { MEMORY_KEYS, useEntity, useEntityJournalDays, useMemories } from "./queries";
import { RenameDialog } from "./RenameDialog";
import { useForget } from "./useForget";
import { useRefile } from "./useRefile";

/** One person, place or thing, and what Nocturne remembers about them. */
export function EntityPage({ id }: { id: string }) {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const entity = useEntity(id);
  const memories = useMemories({ entityId: id });
  const journalDays = useEntityJournalDays(id, entity.data ? entity.data.kind !== "thing" : false);
  const heading = useRef<HTMLHeadingElement>(null);
  const journalHeading = useId();
  const [busy, setBusy] = useState(false);
  const forget = useForget(() => heading.current?.focus());
  const refile = useRefile();
  const [renaming, setRenaming] = useState(false);

  const [refocus, setRefocus] = useState(0);
  // After the dialog has gone and given focus back to what opened it, so the new name is read.
  useEffect(() => {
    if (refocus > 0) heading.current?.focus();
  }, [refocus]);

  const closeRename = () => {
    setRenaming(false);
    setRefocus((n) => n + 1);
  };

  const page = (node: ReactNode) => (
    <>
      {node}
      {forget.dialog}
      {refile.dialog}
    </>
  );

  if (entity.isError || memories.isError) {
    return page(
      <div className={styles.message} role="alert">
        <h1 className="t-page-title">{t("memories.entity.missing")}</h1>
        <BackLink to="/memories" label={t("screen.memories.title")} />
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

  const undo = async (changeId: string, message: string, back?: string) => {
    try {
      await engine.undoChange(changeId);
      refresh(...MEMORY_KEYS);
      if (back) navigate(back);
      toast.show({ message });
    } catch {
      refresh(...MEMORY_KEYS);
      toast.show({ message: t("memories.failed") });
    }
  };

  const change = e.lastChange;

  return page(
    <div className={styles.entity}>
      <div className={styles.back}>
        <BackLink to="/memories" label={t("screen.memories.title")} />
      </div>
      <header className={styles.entityHead}>
        <div>
          <h1 ref={heading} tabIndex={-1} className={`t-page-title ${styles.name}`}>
            {e.name}
          </h1>
          <p className={styles.summary}>
            {e.role ? t("memories.entity.role", { role: e.role, summary }) : summary}
          </p>
          {e.formerNames.length > 0 ? (
            <p className={styles.summary}>
              {t("memories.entity.alsoCalled", {
                names: new Intl.ListFormat("en-GB", { style: "long", type: "conjunction" }).format(
                  e.formerNames,
                ),
              })}
            </p>
          ) : null}
        </div>
        <div className={styles.headActions}>
          <Button variant="quiet" flush disabled={busy} onClick={() => setRenaming(true)}>
            {t("memories.entity.rename")}
          </Button>
          <Button
            variant="quiet"
            flush
            disabled={busy || toMark.length === 0}
            onClick={() => void markAllPrivate()}
          >
            {toMark.length === 0 && rows.length > 0
              ? t("memories.allPrivate")
              : t("memories.markAllPrivate")}
          </Button>
          {change && change.kind !== "refile" ? (
            <Button
              variant="quiet"
              flush
              onClick={() => void undo(change.id, t("memories.entity.undone"))}
            >
              {change.kind === "rename"
                ? t("memories.entity.undoRename", { name: change.from })
                : t("memories.entity.undoMerge", { name: change.from })}
            </Button>
          ) : null}
        </div>
      </header>
      <hr className={styles.rule} />
      {rows.length === 0 ? (
        <p className={styles.empty}>{t("memories.entity.none")}</p>
      ) : (
        <>
          <MemoryList
            memories={rows}
            onForget={(m) => forget.open([m.id])}
            onRefile={refile.open}
          />
          <p className={styles.hint}>{t("memories.hint")}</p>
        </>
      )}
      {journalDays.data && journalDays.data.length > 0 ? (
        <section className={styles.journalDays} aria-labelledby={journalHeading}>
          <h2 id={journalHeading} className={styles.journalHead}>
            {t("journal.people.section")}
          </h2>
          <ul className={styles.journalList}>
            {journalDays.data.map((day) => (
              <li key={day}>
                <a className={styles.journalDay} href={`#/journal/${day}`}>
                  {formatDay(day)}
                </a>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {renaming ? (
        <RenameDialog
          entity={e}
          onClose={closeRename}
          onRenamed={(renamed, changeId) => {
            closeRename();
            refresh(...MEMORY_KEYS);
            toast.show({
              message: t("rename.done", { name: renamed.name }),
              action: changeId
                ? {
                    label: t("memories.undo"),
                    onAction: () => void undo(changeId, t("memories.entity.undone")),
                  }
                : undefined,
            });
          }}
          onMerged={(merged) => {
            setRenaming(false);
            refresh(...MEMORY_KEYS);
            navigate(`/memories/${merged.entity.id}`);
            toast.show({
              message: t("rename.merge.done", { name: merged.entity.name }),
              action: {
                label: t("memories.undo"),
                onAction: () =>
                  void undo(merged.changeId, t("memories.entity.undone"), `/memories/${id}`),
              },
            });
          }}
        />
      ) : null}
    </div>,
  );
}
