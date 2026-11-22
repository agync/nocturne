import { t } from "../../../copy/t";
import { BottomSheet } from "../../screens/Dialog";
import { Button } from "../../screens/ui";

/** The one plain line before the phone's own prompt, and "Continue" that opens it. */
export function VoiceAsk({
  ios,
  onContinue,
  onNotNow,
}: {
  ios: boolean;
  onContinue(): void;
  onNotNow(): void;
}) {
  return (
    <BottomSheet label={t("phone.alone.voice.row")} onClose={onNotNow}>
      <div className="ph-sheet-body ph-stack" style={{ gap: 16 }}>
        <p className="ph-body">
          {ios ? t("phone.alone.voice.iphone") : t("phone.alone.voice.android")}
        </p>
        <Button kind="primary" onClick={onContinue}>
          {t("phone.sources.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNotNow}>
          {t("phone.sources.notNow")}
        </button>
      </div>
    </BottomSheet>
  );
}
