import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";
import Sent from "./Sent";

/** Serves #/settings/online and the pages under it. */
export default function Online() {
  const [setup] = useRoute("/settings/online/setup");
  const [sent] = useRoute("/settings/online/sent");
  if (sent) return <Sent />;
  return <Placeholder title={setup ? "screen.online.setup.title" : "screen.online.title"} />;
}
