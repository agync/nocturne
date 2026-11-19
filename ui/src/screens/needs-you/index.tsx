import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";
import { useLocation } from "wouter";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { MemoryView, NeedsYouEntry, Receipt } from "../../engine/bindings";
import { useEngine, useNeedsYou, usePage, useRefresh } from "../../engine/context";
import { DueEntry, dueBadge, SameNameEntry, UnconfirmedEntry, WhichEntry } from "./Entries";
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
    case "sameName":
      return `sameName:${entry.question.id}`;
  }
}

/** Decisions first, then things to confirm, then things to tick off, as the design lists them. */
const RANK: Record<NeedsYouEntry["type"], number> = {
  which: 0,
  sameName: 1,
  unconfirmed: 2,
  due: 3,
  promise: 4,
};

const isOpen = (entry: NeedsYouEntry) =>
  entry.type === "which" ||
  entry.type === "unconfirmed" ||
  entry.type === "sameName" ||
  !entry.done;

/** What is waiting for the person: decide, confirm or tick off. One at a time, from the keyboard if they like. */
export default function NeedsYou() {
  const engine = useEngine();
  const refresh = useRefresh();
  const toast = useToast();
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
  /** Where focus goes once the forget dialog, which stays open to say it is done, is closed. */
  const afterForget = useRef<number | null>(null);

  const entries = (needs.data ?? [])
    .filter((e) => isOpen(e) && !settled.has(entryKey(e)))
    .sort((a, b) => RANK[a.type] - RANK[b.type]);

  // After an action the entry is gone; focus goes to whoever took its place, never to the page top.
  const focusEntry = useCallback((at: number) => {
    const items = list.current?.querySelectorAll<HTMLElement>(":scope > li");
    const next = items?.[Math.min(at, (items?.length ?? 1) - 1)];
    if (next) next.focus();
    else empty.current?.focus();
  }, []);

  useEffect(() => {
    const at = focusAt.current;
    if (at === null) return;
    focusAt.current = null;
    focusEntry(at);
  }, [entries.length, focusEntry]);

  /** "Mark as done" waits out its toast: entry key -> the memory to mark. */
  const pendingDone = useRef(new Map<string, string>());

  const unsettle = useCallback((key: string) => {
    setSettled((all) => {
      const next = new Set(all);
      next.delete(key);
      return next;
    });
  }, []);

  const commitDone = useCallback(
    (key: string) => {
      const memoryId = pendingDone.current.get(key);
      if (memoryId === undefined) return;
      pendingDone.current.delete(key);
      engine.markDone(memoryId).then(
        () => refresh("needsYou", "status", "page"),
        () => {
          unsettle(key);
          setMessage(t("needs.msg.failed"));
        },
      );
    },
    [engine, refresh, unsettle],
  );

  // A press that is still waiting for its toast to end must not be lost.
  useEffect(() => {
    const flush = () => {
      for (const key of [...pendingDone.current.keys()]) commitDone(key);
    };
    window.addEventListener("pagehide", flush);
    return () => {
      window.removeEventListener("pagehide", flush);
      flush();
    };
  }, [commitDone]);

  const markDone = (key: string, index: number, memory: MemoryView) => {
    pendingDone.current.set(key, memory.id);
    focusAt.current = index;
    setSettled((all) => new Set(all).add(key));
    setMessage(t("needs.msg.done", { subject: memory.subject }));
    toast.show({
      key: `done:${key}`,
      message: t("page.done.toast"),
      action: {
        label: t("page.done.undo"),
        onAction: () => {
          pendingDone.current.delete(key);
          unsettle(key);
          setMessage("");
        },
      },
      onExpire: () => commitDone(key),
    });
  };

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
      <ScreenSheet>
        <div className={styles.message} role="alert">
          <p>{t("needs.error")}</p>
          <button type="button" className={styles.retry} onClick={() => void needs.refetch()}>
            {t("button.tryAgain")}
          </button>
        </div>
      </ScreenSheet>
    );
  }
  if (!needs.data) {
    return (
      <ScreenSheet>
        <p className={styles.message} role="status">
          {t("needs.loading")}
        </p>
      </ScreenSheet>
    );
  }

  const count = entries.length;
  return (
    <ScreenSheet>
      <div className={styles.screen}>
        <header className={styles.header}>
          <div className={styles.headerText}>
            <p className={styles.eyebrow}>{t("needs.eyebrow", { count })}</p>
            <h1 className="t-page-title">{t("needs.title", { count })}</h1>
          </div>
          <p className={styles.keys}>{t("needs.keys")}</p>
        </header>

        <div className={styles.content}>
          {count > 0 ? (
            <ol
              ref={list}
              className={styles.list}
              aria-label={t("needs.list")}
              onKeyDown={onKeyDown}
            >
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
                            const done = await engine.resolveWhich(
                              entry.conflictId,
                              option.memoryId,
                            );
                            return t("needs.msg.which", {
                              subject: done.subject,
                              chosen: done.chosen,
                            });
                          })
                        }
                      />
                    ) : entry.type === "sameName" ? (
                      <SameNameEntry
                        index={index}
                        busy={working}
                        question={entry.question}
                        onOpen={openReceipt}
                        onAnswer={(answer) =>
                          void settle(key, index, async () => {
                            const done = await engine.answerSameName(entry.question.id, answer);
                            refresh("journal");
                            return answer.type !== "leave" && done.role
                              ? t("journal.people.filed", { name: done.name, role: done.role })
                              : "";
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
                        onDone={() => markDone(key, index, entry.memory)}
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
            onClose={() => {
              setForgetting(null);
              const at = afterForget.current;
              afterForget.current = null;
              // The dialog hands focus back to a button that has gone; the next entry takes it.
              if (at !== null) setTimeout(() => focusEntry(at), 0);
            }}
            onDone={() => {
              // Called once, as soon as the engine has forgotten; the dialog says so until closed.
              const { key } = forgetting;
              afterForget.current = Math.max(
                0,
                entries.findIndex((e) => entryKey(e) === key),
              );
              setSettled((all) => new Set(all).add(key));
              setMessage(t("needs.msg.forgotten"));
              refresh("needsYou", "status", "page", "items", "memories");
            }}
          />
        ) : null}
      </div>
    </ScreenSheet>
  );
}
