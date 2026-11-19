import type { ReactNode } from "react";
import { useRef, useState } from "react";
import { useLocation } from "wouter";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { StatusLabel } from "../../components/StatusLabel/StatusLabel";
import { useToast } from "../../components/Toast/Toast";
import { formatDay, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { CouldntReadReason } from "../../engine/bindings";
import { useEngine, useEngineEvent, useRefresh } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { CheckinPage } from "./CheckinPage";
import styles from "./Item.module.css";
import { kindWord } from "./memoryText";
import { Original } from "./Original";
import { MEMORY_KEYS, useItem, useMemories } from "./queries";
import { useForget } from "./useForget";

const REASON = {
  unsupported: "item.reason.unsupported",
  corrupt: "item.reason.corrupt",
  locked: "item.reason.locked",
  tooLong: "item.reason.tooLong",
  language: "item.reason.language",
  mindUnavailable: "item.reason.mindUnavailable",
} as const satisfies Record<CouldntReadReason, Parameters<typeof t>[0]>;

/** #/items/:itemId: the original on the left, what Nocturne understood from it on the right. */
export function ItemPage({ id }: { id: string }) {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const refresh = useRefresh();
  const [, navigate] = useLocation();
  const item = useItem(id);
  const memories = useMemories({ itemId: id });
  const heading = useRef<HTMLHeadingElement>(null);
  const [busy, setBusy] = useState(false);
  const forget = useForget(
    (report) => {
      // With the original gone there is nothing left to look at.
      if (report.originalsRemoved > 0) navigate("/memories");
      else heading.current?.focus();
    },
    [id],
  );

  useEngineEvent("item-changed", (changed) => {
    if (changed.id === id) refresh("item");
  });

  const page = (node: ReactNode) => (
    <>
      {node}
      {forget.dialog}
    </>
  );

  if (item.isError || memories.isError) {
    return page(
      <ScreenSheet>
        <BackLink to="/memories" label={t("screen.memories.title")} />
        <div className={styles.message} role="alert">
          <h1 className="t-page-title">{t("item.missing")}</h1>
        </div>
      </ScreenSheet>,
    );
  }
  if (!item.data || !memories.data) {
    return page(
      <ScreenSheet>
        <p className={styles.message} role="status">
          {t("item.loading")}
        </p>
      </ScreenSheet>,
    );
  }

  const it = item.data;
  if (it.journal === "checkin" && it.checkin) {
    return page(<CheckinPage item={it} checkin={it.checkin} />);
  }
  const list = memories.data;
  const keptForever = list.length > 0 && list.every((m) => m.keepForever);
  const highlights = list.flatMap((m) =>
    m.receipts.flatMap((r) =>
      r.itemId === id && r.span ? [{ span: r.span, sentence: m.sentence }] : [],
    ),
  );

  const run = async (work: () => Promise<unknown>, message: string) => {
    setBusy(true);
    try {
      await work();
      refresh(...MEMORY_KEYS);
      toast.show({ message });
    } catch {
      toast.show({ message: t("item.failed") });
    } finally {
      setBusy(false);
    }
  };

  const seconds =
    it.understoodInMs === null ? null : Math.max(1, Math.round(it.understoodInMs / 1000));

  return page(
    <ScreenSheet flush>
      <div className={styles.card}>
        <div className={styles.original}>
          <Original item={it} highlights={highlights} />
        </div>
        <div className={styles.details}>
          <BackLink to="/memories" label={t("screen.memories.title")} />
          <p className={`t-section ${styles.meta}`}>
            {t("item.meta", {
              kind: kindWord(it.kind),
              day: formatDay(it.keptAt),
              time: formatTime(it.keptAt),
            })}
          </p>
          <h1 ref={heading} tabIndex={-1} className={`t-item-title ${styles.title}`}>
            {it.title}
          </h1>
          <div className={styles.status}>
            {it.status === "unconfirmed" ? <StatusLabel status="unconfirmed" /> : null}
            {seconds !== null && it.status !== "partlyUnderstood" ? (
              <span className={styles.understood}>
                <span className={styles.check} aria-hidden="true" />
                {t("item.understood", { count: seconds })}
              </span>
            ) : it.status === "unconfirmed" ? null : (
              <StatusLabel status={it.status} tonight={it.status === "kept"} />
            )}
          </div>
          {it.status === "couldntRead" && it.reason ? (
            <p className={styles.reason}>{t(REASON[it.reason])}</p>
          ) : null}

          {it.summary || it.text ? (
            <section className={styles.says} aria-labelledby="says">
              <h2 id="says" className="t-section">
                {t(it.kind === "audio" ? "item.says.audio" : "item.says")}
              </h2>
              <p>{it.summary ?? it.text}</p>
            </section>
          ) : null}

          <section className={styles.from} aria-labelledby="from">
            <h2 id="from" className="t-section">
              {t("item.memories", { count: list.length })}
            </h2>
            {list.length === 0 ? (
              <p className={styles.none}>{t("item.memories.none")}</p>
            ) : (
              <ul>
                {list.map((m) => (
                  <li key={m.id} className={styles.fromRow}>
                    {m.entityId ? (
                      <a className="hit" href={`#/memories/${m.entityId}`}>
                        {m.sentence}
                      </a>
                    ) : (
                      <span>{m.sentence}</span>
                    )}
                    {m.status === "unconfirmed" ? <StatusLabel status="needsYou" /> : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <div className={styles.actions}>
            {it.journal ? null : (
              <Button
                variant="secondary"
                aria-pressed={it.private}
                disabled={busy}
                onClick={() =>
                  void run(
                    () => engine.setItemPrivate(id, !it.private),
                    t(it.private ? "item.private.off" : "item.private.on"),
                  )
                }
              >
                {t("item.action.private")}
              </Button>
            )}
            <Button
              variant="secondary"
              aria-pressed={keptForever}
              disabled={busy || list.length === 0}
              onClick={() =>
                void run(
                  () => engine.setItemKeepForever(id, !keptForever),
                  t(keptForever ? "item.keepForever.off" : "item.keepForever.on"),
                )
              }
            >
              {t("item.action.keepForever")}
            </Button>
            <Button
              variant="quiet"
              disabled={busy || it.status === "reading"}
              onClick={() =>
                void run(
                  () => (it.status === "couldntRead" ? engine.retryItem(id) : engine.readNow(id)),
                  t("item.readAgain.started"),
                )
              }
            >
              {t("item.action.readAgain")}
            </Button>
            {it.hasOriginal && it.originalPath ? (
              <Button
                variant="quiet"
                onClick={() =>
                  platform
                    .openOriginal(id, it.originalPath as string)
                    .catch(() => toast.show({ message: t("notice.action.failed") }))
                }
              >
                {t("item.action.open")}
              </Button>
            ) : null}
            <Button
              variant="forget"
              disabled={list.length === 0 && it.kind !== "checkin"}
              onClick={() => forget.open(list.map((m) => m.id))}
            >
              {t("item.action.forget")}
            </Button>
          </div>
        </div>
      </div>
    </ScreenSheet>,
  );
}
