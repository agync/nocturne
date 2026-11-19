import { Box, MapPin, User } from "lucide-react";
import type { EntityKind } from "../engine/bindings";

const GLYPH = { person: User, place: MapPin, thing: Box } as const;

/** A person, place or thing as a small shape. Decorative: the name always sits beside it. */
export function NameIcon({ kind, size = 14 }: { kind: EntityKind; size?: number }) {
  const Glyph = GLYPH[kind];
  return <Glyph size={size} strokeWidth={1.6} aria-hidden="true" focusable="false" />;
}
