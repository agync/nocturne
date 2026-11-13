import { ScreenSheet } from "../components/ScreenSheet/ScreenSheet";
import { t } from "../copy/t";

export function NotFound() {
  return (
    <ScreenSheet>
      <h1 className="t-page-title">{t("screen.notFound.title")}</h1>
      <p className="t-ui-body" style={{ margin: "var(--s-3) 0", color: "var(--ink2)" }}>
        {t("screen.notFound.body")}
      </p>
      <a href="#/today">{t("screen.notFound.action")}</a>
    </ScreenSheet>
  );
}
