import { invoke } from "@tauri-apps/api/core";
import { t } from "../../../copy/t";
import { Button, Icon } from "../../screens/ui";

/** This app's page in Google Play. */
export const STORE = "https://play.google.com/store/apps/details?id=app.nocturne.phone";

/** Which side is too old for the other, and both versions. */
export interface UpdateNeeded {
  kind: "phone" | "computer";
  computer: string;
  phone: string;
}

/** "Update Nocturne on this phone" or "on your computer", on Today. */
export function UpdateCard({ update }: { update: UpdateNeeded }) {
  const phone = update.kind === "phone";
  return (
    <section className="ph-card ph-update" aria-labelledby="ph-update-title">
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
            void invoke("plugin:opener|open_url", { url: STORE }).catch(() => undefined)
          }
        >
          {t("phone.app.update.play")}
        </Button>
      ) : null}
      <p className="ph-rc">
        {t("phone.app.update.versions", { computer: update.computer, phone: update.phone })}
      </p>
    </section>
  );
}
