import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";

/** Stands in until the journaling build replaces it. */
export default function Journal() {
  return (
    <ScreenSheet>
      <h1 className="t-page-title">{t("screen.journal.title")}</h1>
    </ScreenSheet>
  );
}
