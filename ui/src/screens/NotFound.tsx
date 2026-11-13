import { t } from "../copy/t";

export function NotFound() {
  return (
    <section style={{ padding: "var(--s-6) var(--s-5)" }}>
      <h1 className="t-page-title">{t("screen.notFound.title")}</h1>
      <p className="t-ui-body" style={{ margin: "var(--s-3) 0", color: "var(--ink2)" }}>
        {t("screen.notFound.body")}
      </p>
      <a href="#/today">{t("screen.notFound.action")}</a>
    </section>
  );
}
