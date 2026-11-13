import { createRoot } from "react-dom/client";
import { Mark } from "../components/Mark/Mark";
import { t } from "../copy/t";
import "../design/global.css";

/** The phone app, served by the computer to the person's paired phones. */
function Phone() {
  return (
    <main>
      <Mark state="resting" decorative />
      <h1 className="t-page-title">{t("phone.name")}</h1>
    </main>
  );
}

createRoot(document.getElementById("root") as HTMLElement).render(<Phone />);
