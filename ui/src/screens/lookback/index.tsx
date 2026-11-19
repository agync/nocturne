import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";

/** Stands in until the journaling build replaces it. */
export default function Lookback() {
  return (
    <ScreenSheet>
      <h1 className="t-page-title">{t("screen.lookback.title")}</h1>
    </ScreenSheet>
  );
}
