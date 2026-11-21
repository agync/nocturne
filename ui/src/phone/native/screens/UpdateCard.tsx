import { invoke } from "@tauri-apps/api/core";
import { t } from "../../../copy/t";
import { Button, Icon } from "../../screens/ui";

/** This app's page in Google Play. */
export const STORE = "https://play.google.com/store/apps/details?id=app.nocturne.phone";
/** Nocturne in the App Store, found by name until the app has its store page. */
export const APP_STORE =
  "itms-apps://search.itunes.apple.com/WebObjects/MZSearch.woa/wa/search?media=software&term=Nocturne";

/** Which side is too old for the other, both versions, and whether this is an iPhone. */
export interface UpdateNeeded {
  kind: "phone" | "computer";
  computer: string;
  phone: string;
  ios?: boolean;
}

/** "Update Nocturne on this phone" or "on your computer", on Today. */
export function UpdateCard({ update }: { update: UpdateNeeded }) {
  const phone = update.kind === "phone";
  return (
    <section className="ph-card ph-update" aria-labelledby="ph-update-title" aria-live="polite">
      <Icon name={phone ? "refresh" : "laptop"} size={26} />
      <h2 className="ph-update-title" id="ph-update-title">
        {phone ? t("phone.app.update.phone.title") : t("phone.app.update.computer.title")}
      </h2>
      <p className="ph-body">
        {phone ? t("phone.app.update.phone.body") : t("phone.app.update.computer.body")}
      </p>
      {phone ? (
        <Button
          kind="primary"
          onClick={() =>
            void invoke("plugin:opener|open_url", {
              url: update.ios ? APP_STORE : STORE,
            }).catch(() => undefined)
          }
        >
          {update.ios ? t("phone.app.update.appStore") : t("phone.app.update.play")}
        </Button>
      ) : null}
      <p className="ph-rc">
        {t("phone.app.update.versions", { computer: update.computer, phone: update.phone })}
      </p>
    </section>
  );
}
