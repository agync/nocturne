import { useCallback, useEffect, useRef, useState } from "react";
import { formatDay, formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { ForgetOriginal, ForgetPreview, ForgetReport } from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine } from "../../engine/context";
import { Button } from "../Button/Button";
import { Checkbox } from "../Checkbox/Checkbox";
import { Dialog } from "../Dialog/Dialog";
import styles from "./ForgetDialog.module.css";

export interface ForgetDialogProps {
  memoryIds: string[];
  itemIds?: string[];
  defaultIncludeOriginals?: boolean;
  onClose: () => void;
  /**
   * Called as soon as the engine has forgotten, even if the dialog has gone by then. The dialog
   * stays open to say so until it is closed.
   */
  onDone: (report: ForgetReport) => void;
}

type Phase = "loading" | "ready" | "working" | "done" | "failedPreview";

/** A token is only good for the choice it was made for, so the two travel together. */
interface Prepared {
  token: string;
  includeOriginals: boolean;
}

const isExpired = (error: unknown): boolean => {
  const e = toEngineError(error);
  return /expire/i.test(`${e.code} ${e.detail}`);
};

function originalText(original: ForgetOriginal): string {
  const day = formatDay(original.keptAt);
  if (original.kind === "audio" && original.durationMs !== null) {
    return t("forget.original.audio", {
      kind: t("kind.audio"),
      duration: formatDuration(original.durationMs),
      day,
    });
  }
  return t("forget.original.other", {
    title: original.label,
    kind: t(`kind.${original.kind}.spoken`),
    day,
  });
}

/**
 * Forgetting for good, never by accident: it names every memory and original, says what stays,
 * and puts focus on Keep them so Enter cannot forget. Once forgetting has started it cannot be
 * cancelled, so the dialog cannot be dismissed until the engine has answered.
 */
export function ForgetDialog({
  memoryIds,
  itemIds = [],
  defaultIncludeOriginals = false,
  onClose,
  onDone,
}: ForgetDialogProps) {
  const engine = useEngine();
  const keep = useRef<HTMLElement | null>(null);
  const [phase, setPhase] = useState<Phase>("loading");
  const [listing, setListing] = useState<ForgetPreview | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  /** What the box shows. Only `prepared` says what a press of the button would do. */
  const [wanted, setWanted] = useState(defaultIncludeOriginals);
  const [refreshed, setRefreshed] = useState(false);
  const [failed, setFailed] = useState(false);
  const [report, setReport] = useState<ForgetReport | null>(null);
  const ids = useRef<string[]>(memoryIds);
  const alive = useRef(true);
  /** Only the newest request may change what is on screen. */
  const latest = useRef(0);
  const doneRef = useRef(onDone);
  doneRef.current = onDone;
  const idsKey = `${memoryIds.join(",")}|${itemIds.join(",")}`;

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  /**
   * Makes the list (always with originals, so every one can be shown) and a token for `include`.
   * Everything is locked while it runs, so the choice cannot change underneath it.
   */
  const prepare = useCallback(
    async (include: boolean) => {
      const mine = ++latest.current;
      setPhase("loading");
      setWanted(include);
      try {
        const all = new Set(ids.current);
        for (const itemId of itemIds) {
          const found = await engine.listMemories({
            entityId: null,
            itemId,
            status: null,
            query: null,
            limit: null,
          });
          for (const m of found) all.add(m.id);
        }
        const list = [...all];
        ids.current = list;
        if (list.length === 0) {
          if (mine !== latest.current || !alive.current) return;
          setListing(null);
          setPrepared(null);
          setPhase("ready");
          return;
        }
        const full = await engine.previewForget(list, true);
        const own = include ? full : await engine.previewForget(list, false);
        if (mine !== latest.current || !alive.current) return;
        setListing(full);
        setPrepared({ token: own.token, includeOriginals: include });
        setPhase("ready");
      } catch {
        if (mine === latest.current && alive.current) setPhase("failedPreview");
      }
    },
    // itemIds is read through idsKey so a new array with the same ids does not reload.
    [engine, idsKey],
  );

  useEffect(() => {
    void prepare(defaultIncludeOriginals);
    // The first list is made once; the box asks for its own token.
  }, [prepare]);

  const forget = async () => {
    if (!prepared || phase !== "ready") return;
    setPhase("working");
    setFailed(false);
    try {
      const result = await engine.forget(prepared.token);
      doneRef.current(result);
      if (!alive.current) return;
      setReport(result);
      setPhase("done");
    } catch (error) {
      if (!alive.current) return;
      if (isExpired(error)) {
        setRefreshed(true);
      } else {
        setFailed(true);
      }
      // Either way the list is read again, so what is shown is what a new press would do.
      await prepare(prepared.includeOriginals);
    }
  };

  const removable = listing?.originals.filter((o) => o.removable).length ?? 0;
  const includes = prepared?.includeOriginals ?? wanted;
  const memoryCount = listing?.memories.length ?? memoryIds.length;
  const things = memoryCount + (includes ? removable : 0);
  const buttonText =
    includes && removable > 0
      ? t("forget.button.things", { count: things })
      : t("forget.button.memories", { count: memoryCount });
  const hasOriginals = (listing?.originals.length ?? 0) > 0;
  const idle = phase === "ready";

  return (
    <Dialog
      title={t("forget.title", { count: includes && removable > 0 ? things : memoryCount })}
      initialFocus={keep}
      locked={phase === "working"}
      onClose={onClose}
    >
      <div className={styles.body}>
        {phase === "done" && report ? (
          <>
            <div role="status" className={styles.body}>
              <p className={styles.gone}>{t("forget.done.lead")}</p>
              <p className={styles.summary}>
                {[
                  t("forget.done.memories", { count: report.memoriesRemoved }),
                  report.originalsRemoved > 0
                    ? t("forget.done.originalGone", { count: report.originalsRemoved })
                    : hasOriginals
                      ? t("forget.done.originalKept")
                      : "",
                  t("forget.done.tail"),
                ]
                  .filter(Boolean)
                  .join(" ")}
              </p>
            </div>
            <div className={styles.actions}>
              <Button variant="primary" onClick={onClose}>
                {t("button.ok")}
              </Button>
            </div>
          </>
        ) : (
          <>
            <p className={styles.lead}>{t("forget.body")}</p>
            {phase === "loading" && !listing ? (
              <p className={styles.note} role="status">
                {t("forget.loading")}
              </p>
            ) : null}
            {phase === "failedPreview" ? (
              <p className={styles.problem} role="alert">
                {t("forget.error.preview")}
              </p>
            ) : null}
            {refreshed && idle ? (
              <p className={styles.note} role="status">
                {t("forget.refreshed")}
              </p>
            ) : null}
            {failed && idle ? (
              <p className={styles.problem} role="alert">
                {t("forget.error.forget")}
              </p>
            ) : null}
            {listing === null && idle ? <p className={styles.note}>{t("forget.nothing")}</p> : null}
            {listing ? (
              <>
                <ul className={styles.list} aria-label={t("forget.list.label")}>
                  {listing.memories.map((row) => (
                    <li key={row.memoryId} className={styles.row}>
                      <span className={`t-section ${styles.kind}`}>{t("forget.row.memory")}</span>
                      <span className={styles.sentence}>{row.sentence}</span>
                    </li>
                  ))}
                  {listing.originals.map((original) => (
                    <li key={original.itemId} className={styles.row}>
                      <span className={`t-section ${styles.kind}`}>{t("forget.row.original")}</span>
                      <span>
                        <span className={styles.sentence}>{originalText(original)}</span>
                        {original.removable ? null : (
                          <span className={styles.note}>{` ${t("forget.original.blocked")}`}</span>
                        )}
                      </span>
                    </li>
                  ))}
                </ul>
                {removable > 0 ? (
                  <Checkbox
                    label={t("forget.originals.also", { count: removable })}
                    checked={wanted}
                    disabled={!idle}
                    onChange={(next) => {
                      setRefreshed(false);
                      void prepare(next);
                    }}
                  />
                ) : null}
                <div className={styles.stays}>
                  {hasOriginals && !(prepared?.includeOriginals && removable > 0) ? (
                    <p className={styles.note}>{t("forget.stays.original")}</p>
                  ) : null}
                  <p className={styles.note}>{t("forget.stays.out")}</p>
                </div>
              </>
            ) : null}
          </>
        )}
        {phase !== "done" ? (
          <div
            className={styles.actions}
            ref={(el) => {
              // Set while the dialog mounts, so Keep them can take the first focus.
              keep.current = el?.querySelector("button") ?? null;
            }}
          >
            <Button variant="secondary" disabled={phase === "working"} onClick={onClose}>
              {t("forget.keep")}
            </Button>
            {phase === "failedPreview" ? (
              <Button variant="secondary" onClick={() => void prepare(wanted)}>
                {t("button.tryAgain")}
              </Button>
            ) : listing ? (
              <Button
                variant="forget"
                confirming
                disabled={!idle || !prepared}
                aria-busy={phase === "working"}
                onClick={() => void forget()}
              >
                {phase === "working" ? t("forget.working") : buttonText}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}
