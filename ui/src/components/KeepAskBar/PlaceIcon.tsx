import { Globe, Laptop } from "lucide-react";

/** A globe for online and a laptop for this computer, always beside words. */
export function PlaceIcon({ place, size = 14 }: { place: "online" | "computer"; size?: number }) {
  const Glyph = place === "online" ? Globe : Laptop;
  return <Glyph size={size} strokeWidth={1.7} aria-hidden="true" focusable="false" />;
}
