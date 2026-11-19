import type { FeelingPart } from "../engine/bindings";
import { PARTS } from "./feelings";

/** The strokes of each shape and of the sun, on a 24 point grid. */
const PATHS = {
  wound: <path d="M3 15l4.5-6 4.5 6 4.5-6 4.5 6" />,
  lit: (
    <>
      <circle cx="12" cy="12" r="3.5" />
      <path d="M12 2.5v3.5M12 18v3.5M2.5 12H6M18 12h3.5" />
    </>
  ),
  low: (
    <>
      <path d="M4 6l16 11" />
      <path d="M13 17h7v-6" />
    </>
  ),
  settled: <path d="M2.5 13c3-3.5 6.5 3.5 9.5 0s6.5 3.5 9.5 0" />,
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M2 12h2M20 12h2" />
      <path d="m4.93 4.93 1.41 1.41M17.66 17.66l1.41 1.41M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
    </>
  ),
} as const;

const INK: Record<FeelingPart, string> = {
  woundUp: "var(--part-wound-ink)",
  litUp: "var(--part-lit-ink)",
  low: "var(--part-low-ink)",
  settled: "var(--part-settled-ink)",
};

interface FeelingMarkProps {
  /** A part of the field, or the sun that marks a good thing. */
  mark: FeelingPart | "sun";
  size?: number;
  /** Draw in the part's own ink. Off, the mark takes the surrounding text colour. */
  tone?: boolean;
  className?: string;
}

/**
 * The shape of a feeling: a zigzag, rays, a falling line or a flat wave. Colour never carries the
 * meaning alone, so a part's name or its words sit beside the mark. Decorative, hidden from
 * assistive technology.
 */
export function FeelingMark({ mark, size = 16, tone = true, className }: FeelingMarkProps) {
  const shape = mark === "sun" ? "sun" : PARTS[mark].shape;
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={(1.5 * 24) / size}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      data-mark={mark}
      style={tone && mark !== "sun" ? { color: INK[mark] } : undefined}
    >
      {PATHS[shape]}
    </svg>
  );
}
