import {
  Activity,
  Calendar,
  Ellipsis,
  FileText,
  Footprints,
  Gauge,
  Globe,
  Heart,
  Image,
  Mic,
  Moon,
  Pencil,
  Plus,
  Smartphone,
  Waves,
} from "lucide-react";

const GLYPHS = {
  calendar: Calendar,
  more: Ellipsis,
  file: FileText,
  web: Globe,
  image: Image,
  mic: Mic,
  pen: Pencil,
  plus: Plus,
  phone: Smartphone,
  moon: Moon,
  walk: Footprints,
  swim: Waves,
  workout: Activity,
  heart: Heart,
  pressure: Gauge,
} as const;

export type GlyphName = keyof typeof GLYPHS;

/** The small shapes the journal draws, always beside words or named by the control around them. */
export function Glyph({ name, size = 16 }: { name: GlyphName; size?: number }) {
  const Shape = GLYPHS[name];
  return <Shape size={size} strokeWidth={1.5} aria-hidden="true" focusable="false" />;
}
