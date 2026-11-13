import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";
import { useLocation } from "wouter";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { t } from "../../copy/t";
import type { MemoryView, NeedsYouEntry, Receipt } from "../../engine/bindings";
import { useEngine, useNeedsYou, usePage, useRefresh } from "../../engine/context";
import { DueEntry, dueBadge, UnconfirmedEntry, WhichEntry } from "./Entries";
import styles from "./NeedsYou.module.css";

/** One stable key per entry, so a settled entry stays settled whatever the engine says next. */
export function entryKey(entry: NeedsYouEntry): string {
  switch (entry.type) {
    case "which":
      return `which:${entry.conflictId}`;
    case "unconfirmed":
      return `unconfirmed:${entry.memory.id}`;
    case "due":
      return `due:${entry.memory.id}`;
    case "promise":
      return `promise:${entry.memory.id}`;
  }
}

/** Decisions first, then things to confirm, then things to tick off, as the design lists them. */
const RANK: Record<NeedsYouEntry["type"], number> = {
  which: 0,
  unconfirmed: 1,
  due: 2,
  promise: 3,
};

const isOpen = (entry: NeedsYouEntry) =>
  entry.type === "which" || entry.type === "unconfirmed" || !entry.done;

/** What is waiting for the person: decide, confirm or tick off. One at a time, from the keyboard if they like. */
export default function NeedsYou() {
  const engine = useEngine();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const needs = useNeedsYou();
  const { data: page } = usePage();
  const [settled, setSettled] = useState<ReadonlySet<string>>(new Set());
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<{ key: string; memory: MemoryView } | null>(null);
  const list = useRef<HTMLOListElement>(null);
  const empty = useRef<HTMLDivElement>(null);
  const focusAt = useRef<number | null>(null);

  const entries = (needs.data ?? [])
    .filter((e) => isOpen(e) && !settled.has(entryKey(e)))
    .sort((a, b) => RANK[a.type] - RANK[b.type]);

  // After an action the entry is gone; focus goes to whoever took its place, never to the page top.
  useEffect(() => {
    const at = focusAt.current;
    if (at === null) return;
    focusAt.current = null;
    const items = list.current?.querySelectorAll<HTMLElement>(":scope > li");
    const next = items?.[Math.min(at, (items?.length ?? 1) - 1)];
    if (next) next.focus();
    else empty.current?.focus();
  }, [entries.length]);

  const openReceipt = useCallback(
    (receipt: Receipt) => navigate(`/items/${receipt.itemId}`),
    [navigate],
  );

  const settle = async (key: string, index: number, work: () => Promise<string>) => {
    setBusy(key);
    try {
      const said = await work();
      focusAt.current = index;
      setSettled((all) => new Set(all).add(key));
      setMessage(said);
      refresh("needsYou", "status", "page");
    } catch {
      setMessage(t("needs.msg.failed"));
    } finally {
      setBusy(null);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLOListElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable='true']")) return;
    const row = target.closest<HTMLElement>("li[data-entry]");
    if (!row || !list.current) return;
    const rows = [...list.current.querySelectorAll<HTMLElement>(":scope > li")];
    const at = rows.indexOf(row);
    const key = event.key.toLowerCase();
    if (key === "j" || key === "k") {
      event.preventDefault();
      rows[Math.max(0, Math.min(rows.length - 1, at + (key === "j" ? 1 : -1)))]?.focus();
    } else if (/^[1-9]$/.test(key)) {
      const choices = row.querySelectorAll<HTMLButtonElement>("[data-choice]");
      const choice = choices[Number(key) - 1];
      if (choice && !choice.disabled) {
        event.preventDefault();
        choice.click();
      }
    }
  };

  if (needs.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("needs.error")}</p>
        <button type="button" className={styles.retry} onClick={() => void needs.refetch()}>
          {t("button.tryAgain")}
        </button>
      </div>
    );
  }
  if (!needs.data) {
    return (
      <p className={styles.message} role="status">
        {t("needs.loading")}
      </p>
    );
  }

  const count = entries.length;
  return (
    <div className={styles.screen}>
      <header className={styles.header}>
        <div className={styles.headerText}>
          <p className={styles.eyebrow}>{t("needs.eyebrow", { count })}</p>
          <h1 className={styles.title}>{t("needs.title", { count })}</h1>
        </div>
        <p className={styles.keys}>{t("needs.keys")}</p>
      </header>

      <div className={styles.content}>
        {count > 0 ? (
          <ol ref={list} className={styles.list} aria-label={t("needs.list")} onKeyDown={onKeyDown}>
            {entries.map((entry, index) => {
              const key = entryKey(entry);
              const working = busy === key;
              return (
                <li key={key} className={styles.item} data-entry={key} tabIndex={-1}>
                  {entry.type === "which" ? (
                    <WhichEntry
                      index={index}
                      busy={working}
                      title={entry.title}
                      options={entry.options}
                      onOpen={openReceipt}
                      onPick={(option) =>
                        void settle(key, index, async () => {
                          const done = await engine.resolveWhich(entry.conflictId, option.memoryId);
                          return t("needs.msg.which", {
                            subject: done.subject,
                            chosen: done.chosen,
                          });
                        })
                      }
                    />
                  ) : entry.type === "unconfirmed" ? (
                    <UnconfirmedEntry
                      index={index}
                      busy={working}
                      memory={entry.memory}
                      agreeingSources={entry.agreeingSources}
                      used={entry.used}
                      onOpen={openReceipt}
                      onTrue={() =>
                        void settle(key, index, async () => {
                          await engine.confirmMemory(entry.memory.id, true);
                          return t("needs.msg.confirmed", { sentence: entry.memory.sentence });
                        })
                      }
                      onFalse={() => setForgetting({ key, memory: entry.memory })}
                      onLeave={() =>
                        void settle(key, index, async () => {
                          await engine.snooze(entry.memory.id, 7);
                          return t("needs.msg.left");
                        })
                      }
                    />
                  ) : (
                    <DueEntry
                      index={index}
                      busy={working}
                      memory={entry.memory}
                      badge={
                        entry.type === "due"
                          ? dueBadge(entry.due, page?.date)
                          : t("needs.promise.badge")
                      }
                      onOpen={openReceipt}
                      onDone={() =>
                        void settle(key, index, async () => {
                          await engine.markDone(entry.memory.id);
                          return t("needs.msg.done", { subject: entry.memory.subject });
                        })
                      }
                    />
                  )}
                </li>
              );
            })}
          </ol>
        ) : (
          <div ref={empty} className={styles.clear} role="status" tabIndex={-1}>
            <span className={styles.circle} aria-hidden="true" />
            <p className={styles.clearTitle}>{t("needs.empty.title")}</p>
            <p className={styles.clearBody}>{t("needs.empty.body")}</p>
          </div>
        )}
        <p className={styles.said} role="status" aria-live="polite">
          {message}
        </p>
      </div>

      {forgetting ? (
        <ForgetDialog
          memoryIds={[forgetting.memory.id]}
          onClose={() => setForgetting(null)}
          onDone={() => {
            const { key } = forgetting;
            setForgetting(null);
            const at = entries.findIndex((e) => entryKey(e) === key);
            focusAt.current = Math.max(0, at);
            setSettled((all) => new Set(all).add(key));
            setMessage(t("needs.msg.forgotten"));
            refresh("needsYou", "status", "page");
          }}
        />
      ) : null}
    </div>
  );
}
