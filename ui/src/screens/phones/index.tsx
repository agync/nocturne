import { useRoute } from "wouter";
import { CameIn } from "./CameIn";
import { Sources } from "./Sources";

/** From your phones, and the list of what one phone sent. */
export default function Phones() {
  const [cameIn, params] = useRoute("/settings/phones/came-in/:deviceId/:source");
  if (cameIn && params) {
    return <CameIn deviceId={params.deviceId} source={params.source} />;
  }
  return <Sources />;
}
