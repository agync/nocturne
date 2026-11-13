import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";

/** Serves #/settings and #/settings/advanced. */
export default function Settings() {
  const [advanced] = useRoute("/settings/advanced");
  return <Placeholder title={advanced ? "screen.advanced.title" : "screen.settings.title"} />;
}
