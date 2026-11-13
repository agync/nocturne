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
  /** Called as soon as the engine has forgotten. The dialog stays open to say so until closed. */
  onDone: (report: ForgetReport) => void;
}

type Phase = "loading" | "ready" | "working" | "done" | "failed" | "failedPreview";

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
 * and puts focus on Keep them so Enter cannot forget.
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
  const [token, setToken] = useState<string | null>(null);
  const [include, setInclude] = useState(defaultIncludeOriginals);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshed, setRefreshed] = useState(false);
  const [report, setReport] = useState<ForgetReport | null>(null);
  const ids = useRef<string[]>(memoryIds);
  const alive = useRef(true);
  const idsKey = `${memoryIds.join(",")}|${itemIds.join(",")}`;

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  /** The rows always come from a preview that includes originals, so every one can be shown. */
  const load = useCallback(
    async (withOriginals: boolean) => {
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
      if (list.length === 0) return null;
      const full = await engine.previewForget(list, true);
      const own = withOriginals ? full : await engine.previewForget(list, false);
      return { full, token: own.token };
    },
    // itemIds is read through idsKey so a new array with the same ids does not reload.
    [engine, idsKey],
  );

  const start = useCallback(
    async (withOriginals: boolean) => {
      setPhase("loading");
      try {
        const loaded = await load(withOriginals);
        if (!alive.current) return;
        if (loaded === null) {
          setListing(null);
          setPhase("ready");
          return;
        }
        setListing(loaded.full);
        setToken(loaded.token);
        setPhase("ready");
      } catch {
        if (alive.current) setPhase("failedPreview");
      }
    },
    [load],
  );

  useEffect(() => {
    void start(defaultIncludeOriginals);
    // The first list is made once; changes to the include box fetch their own token.
  }, [start]);

  const toggle = async (next: boolean) => {
    setInclude(next);
    setRefreshing(true);
    try {
      const own = await engine.previewForget(ids.current, next);
      if (!alive.current) return;
      setToken(own.token);
      if (next) setListing(own);
      setRefreshing(false);
    } catch {
      if (!alive.current) return;
      setRefreshing(false);
      setPhase("failedPreview");
    }
  };

  const forget = async () => {
    if (!token) return;
    setPhase("working");
    try {
      const result = await engine.forget(token);
      if (!alive.current) return;
      setReport(result);
      setPhase("done");
      onDone(result);
    } catch (error) {
      if (!alive.current) return;
      if (isExpired(error)) {
        setRefreshed(true);
        await start(include);
      } else {
        setPhase("failed");
      }
    }
  };

  const removable = listing?.originals.filter((o) => o.removable).length ?? 0;
  const memoryCount = listing?.memories.length ?? memoryIds.length;
  const things = memoryCount + (include ? removable : 0);
  const buttonText =
    include && removable > 0
      ? t("forget.button.things", { count: things })
      : t("forget.button.memories", { count: memoryCount });
  const hasOriginals = (listing?.originals.length ?? 0) > 0;

  return (
    <Dialog title={t("forget.title", { count: memoryCount })} initialFocus={keep} onClose={onClose}>
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
            {phase === "loading" ? (
              <p className={styles.note} role="status">
                {t("forget.loading")}
              </p>
            ) : null}
            {phase === "failedPreview" ? (
              <p className={styles.problem} role="alert">
                {t("forget.error.preview")}
              </p>
            ) : null}
            {refreshed && phase === "ready" ? (
              <p className={styles.note} role="status">
                {t("forget.refreshed")}
              </p>
            ) : null}
            {phase === "failed" ? (
              <p className={styles.problem} role="alert">
                {t("forget.error.forget")}
              </p>
            ) : null}
            {listing === null && phase === "ready" ? (
              <p className={styles.note}>{t("forget.nothing")}</p>
            ) : null}
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
                        <Checkbox
                          label={originalText(original)}
                          checked={original.removable && include}
                          disabled={!original.removable || refreshing}
                          hint={original.removable ? undefined : t("forget.original.blocked")}
                          onChange={(next) => void toggle(next)}
                        />
                      </span>
                    </li>
                  ))}
                </ul>
                <div className={styles.stays}>
                  {hasOriginals && removable > 0 ? (
                    <p className={styles.note}>{t("forget.original.hint")}</p>
                  ) : null}
                  {hasOriginals && !(include && removable > 0) ? (
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
            <Button variant="secondary" onClick={onClose}>
              {t("forget.keep")}
            </Button>
            {phase === "failedPreview" ? (
              <Button variant="secondary" onClick={() => void start(include)}>
                {t("button.tryAgain")}
              </Button>
            ) : listing ? (
              <Button
                variant="forget"
                confirming
                disabled={phase !== "ready" && phase !== "failed" ? true : refreshing || !token}
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
