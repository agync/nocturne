import { t } from "../../../copy/t";
import { useHeading } from "../../native/screens/parts";
import { openAppSettings } from "../../native/sources";
import { BackBar, Button, Icon, Page } from "../../screens/ui";

/** "Your phone's permissions": what the phone still allows Nocturne, and where to take it back. */
export function Permissions({
  ios,
  fromWelcome = false,
  onBack,
}: {
  ios: boolean;
  /** Opened from Welcome after Delete everything, where there is no This phone to go back to. */
  fromWelcome?: boolean;
  onBack(): void;
}) {
  const heading = useHeading("permissions");
  const bar = fromWelcome ? (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onBack}
        aria-label={t("restore.back")}
      >
        <Icon name="back" />
      </button>
    </header>
  ) : (
    <BackBar title={t("phone.sheet.title")} onBack={onBack} />
  );
  return (
    <Page bar={bar}>
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.permissions.title")}
        </h1>
        <p className="ph-body ph-ink2">
          {ios ? t("phone.alone.permissions.iphone") : t("phone.alone.permissions.android")}
        </p>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={() => void openAppSettings()}>
          {t("phone.sources.openPhoneSettings")}
        </Button>
      </footer>
    </Page>
  );
}
