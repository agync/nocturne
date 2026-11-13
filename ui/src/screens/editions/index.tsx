import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";
import { Listen } from "./Listen";

/** Serves #/listen/:date and #/print/:date (print has no shell). */
export default function Editions() {
  const [print] = useRoute("/print/:date");
  return print ? <Placeholder title="screen.print.title" /> : <Listen />;
}
