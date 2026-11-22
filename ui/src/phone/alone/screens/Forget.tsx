import { useCallback, useEffect, useState } from "react";
import { formatDay, formatDuration } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { ForgetOriginal, ForgetPreview, ForgetReport } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import { HomeRequestError } from "../api";
import { Done } from "./Backup";

type Load =
  | { name: "loading" }
  | { name: "ready"; preview: ForgetPreview }
  | { name: "working"; preview: ForgetPreview }
  | { name: "nothing" }
  | { name: "failedPreview" }
  | { name: "done"; report: ForgetReport };

function originalText(original: ForgetOriginal): string {
  const day = formatDay(original.keptAt);
  if (original.kind === "audio" && original.durationMs !== null)
    return t("forget.original.audio", {
      kind: t("kind.audio"),
      duration: formatDuration(original.durationMs),
      day,
    });
  return t("forget.original.other", {
    title: original.label,
    kind: t(`kind.${original.kind}.spoken`),
    day,
  });
}

/**
 * "Forget…" and "Delete…" from a thing's page: the computer's confirmation, worded for this phone.
 * Forget takes what Nocturne learned from it and keeps the thing; Delete takes the thing too.
 */
export function ForgetItem({
  id,
  withOriginal,
  preview,
  forget,
  onBack,
  onDone,
}: {
  id: string;
  withOriginal: boolean;
  preview(id: string, withOriginal: boolean): Promise<ForgetPreview>;
  forget(token: string): Promise<ForgetReport>;
  onBack(): void;
  /** Deleted: the page it came from shows what is left. */
  onDone(gone: boolean): void;
}) {
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const [refreshed, setRefreshed] = useState(false);
  const [failed, setFailed] = useState(false);
  const heading = useHeading(`forget-${withOriginal ? "delete" : "forget"}`);

  const get = useCallback(async () => {
    setLoad({ name: "loading" });
    try {
      setLoad({ name: "ready", preview: await preview(id, withOriginal) });
    } catch (e) {
      const none = e instanceof HomeRequestError && e.code === "notFound";
      setLoad(none ? { name: "nothing" } : { name: "failedPreview" });
    }
  }, [preview, id, withOriginal]);

  useEffect(() => {
    void get();
  }, [get]);

  if (load.name === "done") {
    const { report } = load;
    const body = [
      report.memoriesRemoved > 0
        ? t("forget.done.memories", { count: report.memoriesRemoved })
        : "",
      report.originalsRemoved > 0
        ? t("forget.done.originalGone", { count: report.originalsRemoved })
        : withOriginal
          ? ""
          : t("forget.done.originalKept"),
      t("forget.done.tail"),
    ]
      .filter(Boolean)
      .join(" ");
    return (
      <Done
        title={t("forget.done.lead")}
        body={body}
        button={t("phone.sources.ok")}
        onDone={() => onDone(withOriginal)}
      />
    );
  }

  const shown = load.name === "ready" || load.name === "working" ? load.preview : null;
  const memories = shown?.memories ?? [];
  const removable = shown?.originals.filter((o) => o.removable) ?? [];
  const things = memories.length + (withOriginal ? removable.length : 0);
  const withThings = withOriginal && removable.length > 0;
  const spoken = memories.length > 0 ? memories.length : things;
  const busy = load.name === "working";

  const go = async () => {
    if (load.name !== "ready") return;
    setLoad({ name: "working", preview: load.preview });
    setFailed(false);
    try {
      setLoad({ name: "done", report: await forget(load.preview.token) });
    } catch (e) {
      if (e instanceof HomeRequestError && e.code === "tooLate") setRefreshed(true);
      else setFailed(true);
      await get();
    }
  };

  return (
    <Page
      bar={
        <header className="ph-bar ph-bar--back">
          <button
            type="button"
            className="ph-ib ph-ib--ink"
            onClick={onBack}
            aria-label={t("phone.cancel")}
            disabled={busy}
          >
            <Icon name="close" />
          </button>
        </header>
      }
    >
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {load.name === "nothing"
            ? t("forget.nothing")
            : withThings
              ? t("forget.title.things", { count: things })
              : t("forget.title", { count: memories.length })}
        </h1>
        {load.name === "nothing" ? null : (
          <p className="ph-body ph-ink2">{t("phone.alone.forget.body", { count: spoken })}</p>
        )}
        {load.name === "loading" ? (
          <p className="ph-small" role="status">
            {t("forget.loading")}
          </p>
        ) : null}
        {load.name === "failedPreview" ? (
          <p className="ph-error" role="alert">
            {t("forget.error.preview")}
          </p>
        ) : null}
        {refreshed ? (
          <p className="ph-small" role="status">
            {t("forget.refreshed")}
          </p>
        ) : null}
        {shown && (memories.length > 0 || shown.originals.length > 0) ? (
          <section className="ph-stack" style={{ gap: 8 }} aria-labelledby="ph-forget-list">
            <h2 className="ph-lbl" id="ph-forget-list">
              {t("forget.list.label")}
            </h2>
            <ul className="ph-delete-rows">
              {memories.map((m) => (
                <li key={m.memoryId}>
                  <span className="ph-srow-hint">{t("forget.row.memory")}</span> {m.sentence}
                </li>
              ))}
              {withOriginal
                ? shown.originals.map((o) => (
                    <li key={o.itemId}>
                      <span className="ph-srow-hint">{t("forget.row.original")}</span>{" "}
                      {originalText(o)}
                      {o.removable ? null : (
                        <span className="ph-srow-hint"> {t("forget.original.blocked")}</span>
                      )}
                    </li>
                  ))
                : null}
            </ul>
          </section>
        ) : null}
        {shown && !withOriginal ? <p className="ph-small">{t("forget.stays.original")}</p> : null}
        {load.name !== "nothing" ? <p className="ph-small">{t("forget.stays.out")}</p> : null}
        {failed ? (
          <p className="ph-error" role="alert">
            {t("forget.error.forget")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button onClick={onBack} disabled={busy}>
          {t("forget.keep", { count: Math.max(spoken, 1) })}
        </Button>
        {load.name === "nothing" ? null : (
          <button
            type="button"
            className="ph-btn ph-btn--ember"
            aria-disabled={load.name !== "ready" ? true : undefined}
            onClick={() => void go()}
          >
            {busy
              ? t("forget.working")
              : withThings
                ? t("forget.button.things", { count: things })
                : t("forget.button.memories", { count: memories.length })}
          </button>
        )}
      </footer>
    </Page>
  );
}
