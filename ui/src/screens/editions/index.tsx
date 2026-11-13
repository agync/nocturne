import { useRoute } from "wouter";
import { Placeholder } from "../Placeholder";

/** Serves #/listen/:date and #/print/:date (print has no shell). */
export default function Editions() {
  const [print] = useRoute("/print/:date");
  return <Placeholder title={print ? "screen.print.title" : "screen.listen.title"} />;
}
