import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { useQueryParam } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Checkbox } from "../../components/Checkbox/Checkbox";
import { ForgetDialog } from "../../components/ForgetDialog/ForgetDialog";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { MemoryView } from "../../engine/bindings";
import { useEngine, usePage, useRefresh } from "../../engine/context";
import { fadingMeta, letGoMeta } from "../memories/memoryText";
import styles from "./Fading.module.css";

const FADE_MS = 400;
const KEEP_DAYS = 30;

function reducedMotion(): boolean {
  if (typeof matchMedia !== "function") return true;
  return matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The id that should take focus once `removed` rows have left: the next row, else the one before. */
export function focusAfter(order: string[], removed: string[]): string | null {
  const first = order.findIndex((id) => removed.includes(id));
  if (first === -1) return null;
  const next = order.slice(first).find((id) => !removed.includes(id));
  if (next) return next;
  return [...order.slice(0, first)].reverse().find((id) => !removed.includes(id)) ?? null;
}

/** Serves #/fading. */
export default function Fading() {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const { data: page } = usePage();
  const show = useQueryParam("show");
  const query = useQuery({ queryKey: ["fading"], queryFn: () => engine.listFading() });
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());
  const [leaving, setLeaving] = useState<ReadonlySet<string>>(new Set());
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  /** Rows with an action under way: they are locked so one press cannot run twice. */
  const [working, setWorking] = useState<ReadonlySet<string>>(new Set());
  const [forgetting, setForgetting] = useState<string[] | null>(null);
  const forgotten = useRef<string[]>([]);
  const inflight = useRef(new Set<string>());
  const pendingFocus = useRef<string | null>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const letGoHeading = useRef<HTMLHeadingElement>(null);
  const alive = useRef(true);
  const fadingId = useId();
  const letGoId = useId();

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const memories = query.data;
  const fading = useMemo(
    () => (memories ?? []).filter((m) => m.status === "fading" && !gone.has(m.id)),
    [memories, gone],
  );
  const letGo = useMemo(
    () => (memories ?? []).filter((m) => m.status === "letGo" && !gone.has(m.id)),
    [memories, gone],
  );

  // Things the engine no longer lists need not be remembered as gone.
  useEffect(() => {
    if (!memories) return;
    const present = new Set(memories.map((m) => m.id));
    setGone((previous) => {
      const next = new Set([...previous].filter((id) => present.has(id)));
      return next.size === previous.size ? previous : next;
    });
  }, [memories]);

  const today = page?.date ?? null;

  // After rows leave, focus goes to the next row, else the one before, else the heading.
  useEffect(() => {
    if (gone.size === 0 && pendingFocus.current === null) return;
    const target = pendingFocus.current;
    if (target === null) return;
    pendingFocus.current = null;
    const row = target === "heading" ? null : document.querySelector(`[data-row-id="${target}"]`);
    const control = row?.querySelector<HTMLElement>("input, button");
    (control ?? title.current)?.focus();
  }, [gone]);

  useEffect(() => {
    if (show === "letGo" && memories) {
      letGoHeading.current?.focus();
      letGoHeading.current?.scrollIntoView?.({ block: "start" });
    }
  }, [show, memories]);

  const leave = useCallback(
    (ids: string[]) => {
      pendingFocus.current =
        focusAfter(
          [...fading, ...letGo].map((m) => m.id),
          ids,
        ) ?? "heading";
      const commit = () => {
        if (!alive.current) return;
        setGone((previous) => new Set([...previous, ...ids]));
        setLeaving((previous) => new Set([...previous].filter((id) => !ids.includes(id))));
        setSelected((previous) => new Set([...previous].filter((id) => !ids.includes(id))));
        refresh("fading", "memories", "entities", "page", "pages", "status", "needsYou");
      };
      if (reducedMotion()) {
        commit();
      } else {
        setLeaving((previous) => new Set([...previous, ...ids]));
        setTimeout(commit, FADE_MS);
      }
    },
    [fading, letGo, refresh],
  );

  const act = useCallback(
    async (work: () => Promise<unknown>, ids: string[], message: string) => {
      if (ids.some((id) => inflight.current.has(id))) return;
      for (const id of ids) inflight.current.add(id);
      setWorking(new Set(inflight.current));
      try {
        await work();
      } catch {
        toast.show({ message: t("fading.announce.failed") });
        return;
      } finally {
        for (const id of ids) inflight.current.delete(id);
        if (alive.current) setWorking(new Set(inflight.current));
      }
      toast.show({ message });
      leave(ids);
    },
    [toast, leave],
  );

  const keepIt = (m: MemoryView) =>
    act(() => engine.snooze(m.id, KEEP_DAYS), [m.id], t("fading.announce.kept"));

  const bringBack = (ids: string[]) =>
    act(() => engine.restore(ids), ids, t("fading.announce.broughtBack", { count: ids.length }));

  const toggle = (id: string, on: boolean) =>
    setSelected((previous) => {
      const next = new Set(previous);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  if (query.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("fading.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  if (!memories) {
    return (
      <p className={styles.message} role="status">
        {t("fading.loading")}
      </p>
    );
  }

  const chosen = letGo.filter((m) => selected.has(m.id)).map((m) => m.id);
  const nothing = fading.length === 0 && letGo.length === 0;

  return (
    <div className={styles.frame}>
      <div className={styles.card}>
        <h1 ref={title} tabIndex={-1} className={`t-page-title ${styles.title}`}>
          {t("fading.title")}
        </h1>
        <p className={`t-ui-body ${styles.explain}`}>{t("fading.explain")}</p>
        <hr className={styles.rule} />

        {nothing ? <p className={styles.empty}>{t("fading.empty.all")}</p> : null}

        {!nothing ? (
          <section className={styles.section} aria-labelledby={fadingId}>
            <div className={styles.head}>
              <h2 id={fadingId} className={`t-section ${styles.heading}`}>
                {t("fading.section.fading")}
              </h2>
            </div>
            {fading.length === 0 ? (
              <p className={styles.empty}>{t("fading.empty.fading")}</p>
            ) : (
              <ul className={styles.list}>
                {fading.map((m) => (
                  <li
                    key={m.id}
                    className={styles.row}
                    data-row-id={m.id}
                    data-leaving={leaving.has(m.id)}
                  >
                    <div className={styles.text}>
                      <span className={styles.sentence}>{m.sentence}</span>
                      <span className={styles.meta}>{fadingMeta(m, today)}</span>
                    </div>
                    <Button
                      variant="quiet"
                      aria-label={t("fading.keepIt.named", { sentence: m.sentence })}
                      aria-disabled={working.has(m.id) || leaving.has(m.id)}
                      onClick={() => void keepIt(m)}
                    >
                      {t("fading.keepIt")}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        ) : null}

        {!nothing ? (
          <section className={styles.section} aria-labelledby={letGoId}>
            <div className={styles.head}>
              <h2
                id={letGoId}
                ref={letGoHeading}
                tabIndex={-1}
                className={`t-section ${styles.heading}`}
              >
                {chosen.length > 0
                  ? t("fading.section.letGo.selected", { count: chosen.length })
                  : t("fading.section.letGo", { count: letGo.length })}
              </h2>
              {chosen.length > 0 ? (
                <div className={styles.bulk}>
                  <Button
                    variant="primary"
                    aria-disabled={chosen.some((id) => working.has(id) || leaving.has(id))}
                    onClick={() => void bringBack(chosen)}
                  >
                    {t("fading.bulk.bringBack", { count: chosen.length })}
                  </Button>
                  <Button variant="forget" onClick={() => setForgetting(chosen)}>
                    {t("fading.bulk.forget", { count: chosen.length })}
                  </Button>
                </div>
              ) : null}
            </div>
            {letGo.length === 0 ? (
              <p className={styles.empty}>{t("fading.empty.letGo")}</p>
            ) : (
              <ul className={styles.list}>
                {letGo.map((m) => (
                  <li
                    key={m.id}
                    className={styles.row}
                    data-row-id={m.id}
                    data-leaving={leaving.has(m.id)}
                  >
                    <Checkbox
                      label={m.sentence}
                      hint={letGoMeta(m)}
                      checked={selected.has(m.id)}
                      onChange={(on) => toggle(m.id, on)}
                    />
                    <Button
                      variant="quiet"
                      aria-label={t("fading.bringBack.named", { sentence: m.sentence })}
                      aria-disabled={working.has(m.id) || leaving.has(m.id)}
                      onClick={() => void bringBack([m.id])}
                    >
                      {t("button.bringBack")}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        ) : null}

        {forgetting ? (
          <ForgetDialog
            memoryIds={forgetting}
            defaultIncludeOriginals
            onDone={() => {
              forgotten.current = forgetting;
            }}
            onClose={() => {
              setForgetting(null);
              const ids = forgotten.current;
              forgotten.current = [];
              if (ids.length > 0) leave(ids);
            }}
          />
        ) : null}
      </div>
    </div>
  );
}
