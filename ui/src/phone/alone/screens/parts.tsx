import { t } from "../../../copy/t";
import { Icon } from "../../screens/ui";

export { Chip, chips, chipsPlain } from "../../screens/chips";

/** "This phone is nearly full.": on Today and on Keep while less than 500 MB is free. */
export function NearlyFull() {
  return (
    <div className="ph-banner" role="status">
      <span className="ph-banner-icon" aria-hidden="true">
        <Icon name="phone" size={22} />
      </span>
      <span>
        <strong>{t("phone.alone.full.title")}</strong> {t("phone.alone.full.body")}
      </span>
    </div>
  );
}
