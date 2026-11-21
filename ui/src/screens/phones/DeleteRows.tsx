import { t } from "../../copy/t";
import type { PhoneDeletePreview, PhoneSource, ReadingKind } from "../../engine/bindings";
import styles from "./Phones.module.css";
import { kindsInline } from "./usePhoneSources";

interface Props {
  source: PhoneSource;
  preview: PhoneDeletePreview;
  phoneName: string;
  /** The kinds of reading the phone brings, for the Health wording. */
  kinds: ReadingKind[];
}

/** What deleting one source of a phone would remove, as the list the person checks. */
export function DeleteRows({ source, preview, phoneName, kinds }: Props) {
  const shared = preview.sharedMemories ?? 0;
  const sharedRow = shared > 0 ? <li>{t("phones.delete.shared", { count: shared })}</li> : null;
  return (
    <ul className={styles.deleteList} aria-label={t("forget.list.label")}>
      {source === "calendar" ? (
        <>
          <li>{t("phones.delete.rows.calendar", { count: preview.items, phone: phoneName })}</li>
          {preview.memories > 0 ? <li>{t("phones.delete.learned")}</li> : null}
          {sharedRow}
          {preview.pageLines > 0 ? (
            <li>{t("phones.delete.lines.calendar", { count: preview.pageLines })}</li>
          ) : null}
        </>
      ) : (
        <>
          <li>
            {t("phones.delete.rows.health", {
              count: preview.items,
              kinds: kindsInline(kinds),
              phone: phoneName,
            })}
          </li>
          {preview.pageLines + preview.recapLines > 0 ? (
            <li>
              {t("phones.delete.lines.health", { count: preview.pageLines + preview.recapLines })}
            </li>
          ) : null}
          {sharedRow}
          {preview.noticing > 0 ? (
            <li>{t("phones.delete.noticing", { count: preview.noticing })}</li>
          ) : null}
        </>
      )}
    </ul>
  );
}
