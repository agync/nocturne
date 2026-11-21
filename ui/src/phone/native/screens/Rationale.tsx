import { t } from "../../../copy/t";
import { Bar, Button, Page } from "../../screens/ui";
import { openPolicy, placesRich, useHeading } from "./parts";

/**
 * How Nocturne uses health readings: the page Health Connect opens from its own link beside
 * Nocturne. Readable without pairing.
 */
export function Rationale({ onBack }: { onBack(): void }) {
  const heading = useHeading("rationale");
  return (
    <Page bar={<Bar />}>
      <main className="ph-main ph-bring">
        <h1 className="ph-h1" tabIndex={-1} ref={heading}>
          {t("phone.rationale.title")}
        </h1>
        <p className="ph-body">{t("phone.rationale.what")}</p>
        <p className="ph-body">{t("phone.rationale.why")}</p>
        <p className="ph-body">{t("phone.rationale.where")}</p>
        <p className="ph-body">{t("phone.rationale.keeping")}</p>
        <p className="ph-body">{placesRich("phone.rationale.stopping")}</p>
        <button type="button" className="ph-native-link" onClick={openPolicy}>
          {t("phone.rationale.link")}
        </button>
        <Button kind="quiet" onClick={onBack}>
          {t("phone.back")}
        </Button>
      </main>
    </Page>
  );
}
