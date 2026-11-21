import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { kindName } from "../../components/health";
import { t } from "../../copy/t";
import type {
  PhoneDeletePreview,
  PhoneSources,
  ReadingKind,
  ReadingRow,
} from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Health.module.css";

interface DeleteReadingsProps {
  phone: PhoneSources;
  /** The kinds the phone brings, and the one the page is on, ticked to begin with. */
  kinds: readonly ReadingKind[];
  current: ReadingKind;
  /** One reading alone, from its row menu: no kinds or dates to choose. */
  single?: ReadingRow | null;
  onClose: () => void;
  /** Called after the readings are gone. */
  onDeleted: () => void;
}

const AND = new Intl.ListFormat("en-GB", { style: "long", type: "conjunction" });

/** "Delete readings": which kinds, all of them or before a day, with what it would take out. */
export function DeleteReadings({
  phone,
  kinds,
  current,
  single = null,
  onClose,
  onDeleted,
}: DeleteReadingsProps) {
  const engine = useEngine();
  const [chosen, setChosen] = useState<ReadingKind[]>([current]);
  const [mode, setMode] = useState<"all" | "before">("all");
  const [date, setDate] = useState("");
  const [preview, setPreview] = useState<PhoneDeletePreview | null>(null);
  const [applying, setApplying] = useState(false);
  const keep = useRef<HTMLElement | null>(null);

  const before = mode === "before" && date !== "" ? date : null;
  const ready = single !== null || (chosen.length > 0 && (mode === "all" || before !== null));

  useEffect(() => {
    let stale = false;
    setPreview(null);
    if (!ready) return;
    engine
      .previewPhoneDelete(
        single
          ? {
              deviceId: phone.deviceId,
              source: "health",
              kinds: null,
              before: null,
              itemId: single.itemId,
            }
          : { deviceId: phone.deviceId, source: "health", kinds: chosen, before },
      )
      .then((answer) => {
        if (!stale) setPreview(answer);
      })
      .catch(() => undefined);
    return () => {
      stale = true;
    };
  }, [engine, phone.deviceId, chosen, before, ready, single]);

  const toggle = (kind: ReadingKind) =>
    setChosen((now) => (now.includes(kind) ? now.filter((k) => k !== kind) : [...now, kind]));

  const apply = async () => {
    if (!preview || applying) return;
    setApplying(true);
    try {
      await engine.deletePhoneData(preview.token, false);
      onDeleted();
    } catch {
      setApplying(false);
    }
  };

  const lines = preview ? preview.pageLines + preview.recapLines : 0;
  const names = AND.format(
    (single ? [current] : kinds.filter((k) => chosen.includes(k))).map((k) =>
      kindName(k).toLowerCase(),
    ),
  );

  return (
    <Dialog
      title={t(single ? "health.delete.title.one" : "health.delete.title")}
      onClose={onClose}
      initialFocus={keep}
      locked={applying}
    >
      <div className={styles.dialogBody}>
        {single ? null : (
          <>
            <fieldset className={styles.chips} aria-label={t("health.delete.kinds")}>
              {kinds.map((kind) => (
                <button
                  key={kind}
                  type="button"
                  className={styles.chip}
                  aria-pressed={chosen.includes(kind)}
                  onClick={() => toggle(kind)}
                >
                  {kindName(kind)}
                </button>
              ))}
            </fieldset>
            <div className={styles.chips}>
              <button
                type="button"
                className={styles.chip}
                aria-pressed={mode === "all"}
                onClick={() => setMode("all")}
              >
                {t("health.delete.all")}
              </button>
              <button
                type="button"
                className={styles.chip}
                aria-pressed={mode === "before"}
                onClick={() => setMode("before")}
              >
                {t("health.delete.before")}
              </button>
              {mode === "before" ? (
                <input
                  type="date"
                  className={styles.date}
                  aria-label={t("health.delete.date")}
                  value={date}
                  onChange={(e) => setDate(e.target.value)}
                />
              ) : null}
            </div>
          </>
        )}
        <ul className={styles.preview} aria-live="polite">
          {preview ? (
            <>
              <li>
                {t("health.delete.readings", {
                  count: preview.items,
                  kinds: names,
                  phone: phone.name,
                })}
              </li>
              {lines > 0 ? <li>{t("health.delete.lines", { count: lines })}</li> : null}
              {(preview.sharedMemories ?? 0) > 0 ? (
                <li>{t("health.delete.shared", { count: preview.sharedMemories ?? 0 })}</li>
              ) : null}
              {preview.noticing > 0 ? (
                <li>{t("health.delete.noticing", { count: preview.noticing })}</li>
              ) : null}
            </>
          ) : null}
        </ul>
        <p className={styles.dialogNote}>
          {t("health.delete.untouched")} {t("forget.stays.out")}
        </p>
        <div className={styles.dialogActions}>
          <span
            ref={(el) => {
              keep.current = el?.querySelector("button") ?? null;
            }}
          >
            <Button variant="secondary" onClick={onClose} disabled={applying}>
              {t("health.delete.keep")}
            </Button>
          </span>
          <Button variant="forget" confirming onClick={apply} disabled={!preview || applying}>
            {t("health.delete.confirm")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
