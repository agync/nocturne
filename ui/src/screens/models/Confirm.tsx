import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { t } from "../../copy/t";
import type { ChangePreview } from "../../engine/bindings";
import { gb, JOB_DOING } from "./jobs";
import styles from "./Models.module.css";

type Kind = "quality" | "download" | "present" | "reindex";

const kindOf = (p: ChangePreview): Kind =>
  p.reindexItems !== null
    ? "reindex"
    : p.downloadBytes > 0
      ? "download"
      : p.smaller
        ? "quality"
        : "present";

const eta = (seconds: number) =>
  t("models.ri.eta", { minutes: Math.max(1, Math.round(seconds / 60)) });

/**
 * M5: what a change does before it happens. The person sees the download, the space, the memory,
 * and says yes or not now. `onConfirm` is only called by the primary button.
 */
export function Confirm({
  preview,
  name,
  fromName,
  onConfirm,
  onClose,
}: {
  preview: ChangePreview;
  name: string;
  fromName: string;
  onConfirm: (allowDownload: boolean) => Promise<void>;
  onClose: () => void;
}) {
  const kind = kindOf(preview);
  const safe = useRef<HTMLElement | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const downloads = preview.downloadBytes > 0;
  const doing = t(JOB_DOING[preview.job]);
  const title =
    kind === "download"
      ? t("models.confirm.titleGet", { name, doing })
      : t("models.confirm.titleUse", { name, doing });
  const memory =
    preview.memoryAfter > preview.memoryNow
      ? "models.facts.memoryUp"
      : preview.memoryAfter < preview.memoryNow
        ? "models.facts.memoryDown"
        : "models.facts.memorySame";

  const body = (() => {
    switch (kind) {
      case "quality":
        return `${t("models.confirm.quality", { from: fromName })}${
          preview.job === "answering" ? ` ${t("models.confirm.photos")}` : ""
        }`;
      case "download":
        return t("models.confirm.download", { from: fromName });
      case "present":
        return t("models.confirm.present");
      case "reindex":
        return t("models.confirm.reread", {
          items: new Intl.NumberFormat("en-GB").format(preview.reindexItems ?? 0),
        });
    }
  })();

  const go = async () => {
    setBusy(true);
    setFailed(false);
    try {
      await onConfirm(downloads);
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  const decline =
    kind === "quality" || kind === "present"
      ? t("models.confirm.keep", { from: fromName })
      : t("models.confirm.notNow");
  const yes =
    kind === "download"
      ? t("models.confirm.get", { size: gb(preview.downloadBytes) })
      : kind === "reindex"
        ? t(downloads ? "models.confirm.getReread" : "models.confirm.rereadOnly")
        : t("models.confirm.use", { name });

  return (
    <Dialog title={title} onClose={onClose} locked={busy} initialFocus={safe}>
      <p className={styles.dialogBody}>{body}</p>
      <dl className={styles.facts}>
        {kind === "reindex" && preview.reindexEtaSeconds !== null ? (
          <>
            <dt>{t("models.facts.reread")}</dt>
            <dd>{t("models.facts.rereadValue", { eta: eta(preview.reindexEtaSeconds) })}</dd>
          </>
        ) : null}
        <dt>{t("models.facts.download")}</dt>
        <dd>
          {downloads
            ? t("models.facts.downloadSize", { size: gb(preview.downloadBytes) })
            : t("models.facts.downloadNone")}
        </dd>
        {downloads ? (
          <>
            <dt>{t("models.facts.space")}</dt>
            <dd>
              {t("models.facts.spaceValue", {
                now: gb(preview.freeNow),
                after: gb(preview.freeAfter),
              })}
            </dd>
          </>
        ) : null}
        <dt>{t("models.facts.memory")}</dt>
        <dd>
          {t(memory, { after: gb(preview.memoryAfter), before: gb(preview.memoryNow) })}
          {preview.fit === "tight"
            ? t("models.facts.tight")
            : preview.fit === "fits"
              ? t("models.facts.fits")
              : ""}
        </dd>
        {kind === "quality" ? (
          <>
            <dt>{t("models.facts.back")}</dt>
            <dd>{t("models.facts.backValue")}</dd>
          </>
        ) : null}
        {preview.newOnly ? (
          <>
            <dt>{t("models.facts.applies")}</dt>
            <dd>{t("models.facts.appliesValue")}</dd>
          </>
        ) : null}
        {preview.tested ? null : (
          <>
            <dt>{t("models.facts.untested")}</dt>
            <dd>{t("models.facts.untestedValue")}</dd>
          </>
        )}
      </dl>
      {downloads ? <p className={styles.hint}>{t("models.confirm.listed")}</p> : null}
      {failed ? (
        <p className={styles.hint} role="alert">
          {t("models.confirm.failed")}
        </p>
      ) : null}
      <div
        className={styles.dialogActions}
        ref={(el) => {
          safe.current = el?.querySelector("button") ?? null;
        }}
      >
        <Button disabled={busy} onClick={onClose}>
          {decline}
        </Button>
        <Button variant="primary" disabled={busy} onClick={() => void go()}>
          {yes}
        </Button>
      </div>
    </Dialog>
  );
}
