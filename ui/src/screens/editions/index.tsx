import { useRoute } from "wouter";
import { Listen } from "./Listen";
import { Print } from "./Print";

/** Serves #/listen/:date and #/print/:date (print has no shell). */
export default function Editions() {
  const [print] = useRoute("/print/:date");
  return print ? <Print /> : <Listen />;
}
