import { t } from "../../copy/t";

/** Shown inside the page region when one screen fails. The shell around it stays usable. */
export function ScreenError() {
  return (
    <section role="alert" style={{ padding: "var(--s-6) var(--s-5)" }}>
      <h1 className="t-page-title">{t("error.screen.title")}</h1>
      <p className="t-ui-body" style={{ margin: "var(--s-3) 0", color: "var(--ink2)" }}>
        {t("error.screen.body")}
      </p>
      <a href="#/today">{t("error.screen.action")}</a>
    </section>
  );
}

/** The last resort, outside every provider: plain words and a way back in. */
export function AppError() {
  return (
    <main style={{ padding: "var(--s-7) var(--s-5)", maxWidth: "40rem", margin: "0 auto" }}>
      <h1 className="t-page-title">{t("error.app.title")}</h1>
      <p className="t-ui-body" style={{ margin: "var(--s-3) 0", color: "var(--ink2)" }}>
        {t("error.app.body")}
      </p>
      <button
        type="button"
        onClick={() => {
          window.location.hash = "#/today";
          window.location.reload();
        }}
        style={{
          minHeight: "var(--size-control)",
          padding: "0 var(--s-4)",
          border: "1px solid var(--rule2)",
          borderRadius: "var(--r-sm)",
          background: "transparent",
          fontWeight: 500,
        }}
      >
        {t("error.app.action")}
      </button>
    </main>
  );
}
