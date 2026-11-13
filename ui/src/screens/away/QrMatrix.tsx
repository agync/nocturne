import type { QrCode } from "../../engine/bindings";
import styles from "./Away.module.css";

interface QrMatrixProps {
  qr: QrCode;
  /** What the picture is, for someone who cannot see it. */
  label: string;
}

const QUIET = 4;

/** Draws the module matrix the computer made: dark squares on white, whatever the theme, so a camera reads it. */
export function QrMatrix({ qr, label }: QrMatrixProps) {
  const { size, modules } = qr;
  let path = "";
  for (let row = 0; row < size; row += 1) {
    let run = -1;
    for (let col = 0; col <= size; col += 1) {
      const dark = col < size && modules[row * size + col] === "1";
      if (dark && run === -1) run = col;
      if (!dark && run !== -1) {
        path += `M${run + QUIET} ${row + QUIET}h${col - run}v1h-${col - run}z`;
        run = -1;
      }
    }
  }
  const side = size + QUIET * 2;
  return (
    <svg
      className={styles.qr}
      viewBox={`0 0 ${side} ${side}`}
      role="img"
      aria-label={label}
      shapeRendering="crispEdges"
      data-size={size}
    >
      <rect width={side} height={side} fill="#fff" />
      <path d={path} fill="#000" />
    </svg>
  );
}
