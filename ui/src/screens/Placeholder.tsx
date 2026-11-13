import type { CopyId } from "../copy/t";
import { t } from "../copy/t";

/** Stands in for a screen a later lane builds. Replace the folder's index, not this file. */
export function Placeholder({ title }: { title: CopyId }) {
  return (
    <section style={{ padding: "var(--s-6) var(--s-5)" }}>
      <h1 className="t-page-title">{t(title)}</h1>
      <p className="t-ui-body" style={{ marginTop: "var(--s-3)", color: "var(--ink2)" }}>
        {t("screen.placeholder")}
      </p>
    </section>
  );
}
