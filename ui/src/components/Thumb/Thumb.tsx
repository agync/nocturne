import type { ItemKind } from "../../engine/bindings";
import styles from "./Thumb.module.css";

interface ThumbProps {
  kind: ItemKind;
  /** Pixel size of the square. */
  size?: number;
  /** The person's own picture, when the engine has one. */
  src?: string;
}

const WAVE = [5, 11, 8, 14, 6];

/** The glyph that tells the kind: photo swatch, waveform, lines, page, open circle for the web. */
export function Thumb({ kind, size = 18, src }: ThumbProps) {
  if (kind === "image" && src) {
    return <img className={styles.image} src={src} alt="" width={size} height={size} />;
  }
  return (
    <span
      className={styles.thumb}
      data-kind={kind}
      aria-hidden="true"
      style={{ "--thumb": `${size}px` } as React.CSSProperties}
    >
      {kind === "audio" &&
        WAVE.map((h, i) => <span key={`${i}-${h}`} className={styles.bar} style={{ height: h }} />)}
      {kind === "text" && (
        <>
          <span className={styles.line} />
          <span className={styles.line} />
          <span className={`${styles.line} ${styles.short}`} />
        </>
      )}
    </span>
  );
}
