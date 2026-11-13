import { useRoute } from "wouter";
import { Advanced } from "./Advanced";
import { Settings } from "./Settings";

/** Serves #/settings and #/settings/advanced. */
export default function SettingsScreen() {
  const [advanced] = useRoute("/settings/advanced");
  return advanced ? <Advanced /> : <Settings />;
}
