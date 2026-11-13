import { useId } from "react";
import { t } from "../../copy/t";
import type { EngineState } from "../../engine/bindings";
import styles from "./Mark.module.css";

export type MarkState = "resting" | "reading" | "tidying" | "needsYou" | "paused";

/** Where the horizon sits, as a fraction of the height. Resting is half; reading is a sliver. */
const HORIZON: Record<MarkState, number> = {
  resting: 0.5,
  reading: 0.72,
  tidying: 0,
  needsYou: 0.5,
  paused: 1,
};

/** B9: how engine states map to the five drawn states. */
export function markStateFor(state: EngineState): MarkState {
  switch (state) {
    case "starting":
    case "gettingReady":
    case "reading":
      return "reading";
    case "resting":
    case "stopped":
      return "resting";
    case "tidying":
      return "tidying";
    case "needsYou":
      return "needsYou";
    case "paused":
      return "paused";
  }
}

interface MarkProps {
  state: MarkState;
  size?: number;
  /** True when words sit beside the mark, so it need not speak. */
  decorative?: boolean;
}

const R = 11.25;

function nightPath(horizon: number): string {
  const y = 24 * horizon;
  const dy = y - 12;
  const w = Math.sqrt(Math.max(0, R * R - dy * dy));
  const large = y < 12 ? 1 : 0;
  return `M ${12 - w} ${y} A ${R} ${R} 0 ${large} 0 ${12 + w} ${y} Z`;
}

/** A circle cut by the horizon. The lower half is night. Drawn, not illustrated. */
export function Mark({ state, size = 20, decorative = false }: MarkProps) {
  const id = useId();
  const horizon = HORIZON[state];
  const stroke = (1.5 * 24) / size;
  const a11y = decorative
    ? { "aria-hidden": true as const }
    : { role: "img", "aria-label": t(`mark.${state}`) };
  return (
    <svg
      className={styles.mark}
      data-state={state}
      data-testid={`mark-${state}`}
      viewBox="0 0 24 24"
      width={size}
      height={size}
      focusable="false"
      {...a11y}
    >
      <title id={id}>{decorative ? "" : t(`mark.${state}`)}</title>
      {state === "tidying" ? (
        <circle
          cx="12"
          cy="12"
          r={R}
          fill="currentColor"
          stroke="currentColor"
          strokeWidth={stroke}
        />
      ) : state === "paused" ? (
        <circle
          cx="12"
          cy="12"
          r={R}
          fill="none"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeDasharray="2.4 2.4"
        />
      ) : (
        <>
          <path d={nightPath(horizon)} fill="currentColor" />
          <circle cx="12" cy="12" r={R} fill="none" stroke="currentColor" strokeWidth={stroke} />
        </>
      )}
    </svg>
  );
}
