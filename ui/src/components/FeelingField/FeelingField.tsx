import { type KeyboardEvent, type PointerEvent, useId, useRef } from "react";
import { t } from "../../copy/t";
import type { FeelingPart, FeelingPoint } from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import { PART_ORDER, PARTS, partOf, strengthStep } from "../../journal/feelings";
import styles from "./FeelingField.module.css";

/** How far an arrow key moves the point, and a larger step with Shift. */
const STEP = 0.1;
const BIG_STEP = 0.25;

/** The middle of a part: milder than a corner, clear of the axes. */
export function middleOf(part: FeelingPart): FeelingPoint {
  const pleasant = part === "litUp" || part === "settled" ? 0.5 : -0.5;
  const energy = part === "litUp" || part === "woundUp" ? 0.5 : -0.5;
  return { pleasant, energy };
}

const clamp = (n: number) => Math.round(Math.min(1, Math.max(-1, n)) * 100) / 100;

interface FeelingFieldProps {
  /** Where the person put the point, or null before they have. */
  point: FeelingPoint | null;
  onChange: (point: FeelingPoint) => void;
}

/**
 * A square field of four parts. A press or a drag places the point; arrow keys move it in small
 * steps. The four parts are also a radio group, so choosing a part never needs the field.
 */
export function FeelingField({ point, onChange }: FeelingFieldProps) {
  const field = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  /** A press already put the point where it landed, so the click that follows must not move it. */
  const placedByPress = useRef(false);
  const helpId = useId();
  const part = point ? partOf(point) : null;

  const at = (event: PointerEvent): FeelingPoint | null => {
    const box = field.current?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return null;
    return {
      pleasant: clamp(((event.clientX - box.left) / box.width) * 2 - 1),
      energy: clamp(1 - ((event.clientY - box.top) / box.height) * 2),
    };
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button > 0) return;
    placedByPress.current = false;
    const placed = at(event);
    if (!placed) return;
    placedByPress.current = true;
    dragging.current = true;
    try {
      field.current?.setPointerCapture?.(event.pointerId);
    } catch {
      // A pointer that has already gone cannot be captured; the press still counts.
    }
    onChange(placed);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    const placed = at(event);
    if (placed) onChange(placed);
  };

  const stopDragging = () => {
    dragging.current = false;
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const arrow = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[
      event.key
    ];
    placedByPress.current = false;
    if (!arrow || event.ctrlKey || event.altKey || event.metaKey) return;
    event.preventDefault();
    const by = event.shiftKey ? BIG_STEP : STEP;
    const from = point ?? { pleasant: 0, energy: 0 };
    onChange({
      pleasant: clamp(from.pleasant + (arrow[0] ?? 0) * by),
      energy: clamp(from.energy + (arrow[1] ?? 0) * by),
    });
  };

  // A part chosen with Space or Enter has no pointer position, so it takes the middle.
  const choose = (chosen: FeelingPart) => {
    if (placedByPress.current) placedByPress.current = false;
    else onChange(middleOf(chosen));
  };

  const dotLeft = point ? `${((point.pleasant + 1) / 2) * 100}%` : undefined;
  const dotTop = point ? `${((1 - point.energy) / 2) * 100}%` : undefined;

  return (
    <div className={styles.wrap}>
      <span className={styles.axis} data-edge="top">
        {t("feeling.axis.moreEnergy")}
      </span>
      {/* biome-ignore lint/a11y/useSemanticElements: the four parts are the field itself, so a fieldset of native radios cannot be laid out as its quadrants */}
      <div
        ref={field}
        className={styles.field}
        role="radiogroup"
        aria-label={t("journal.a11y.field")}
        aria-describedby={helpId}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={stopDragging}
        onPointerCancel={stopDragging}
        onKeyDown={onKeyDown}
      >
        {PART_ORDER.map((id) => (
          // biome-ignore lint/a11y/useSemanticElements: see the group above
          <button
            key={id}
            type="button"
            role="radio"
            aria-checked={part === id}
            className={styles.part}
            data-part={id}
            onClick={() => choose(id)}
          >
            <FeelingMark mark={id} size={16} />
            {t(PARTS[id].name)}
          </button>
        ))}
        {point ? (
          <span
            className={styles.dot}
            data-part={part}
            style={{ left: dotLeft, top: dotTop }}
            aria-hidden="true"
          >
            {part ? <FeelingMark mark={part} size={14} /> : null}
          </span>
        ) : null}
      </div>
      <span className={styles.axis} data-edge="bottom">
        {t("feeling.axis.lessEnergy")}
      </span>
      <span className={styles.axisRow}>
        <span>← {t("feeling.axis.unpleasant")}</span>
        <span>{t("feeling.axis.pleasant")} →</span>
      </span>
      <span id={helpId} className="sr-only">
        {t("journal.a11y.fieldHelp")}
      </span>
      <span className="sr-only" role="status">
        {point && part
          ? t("journal.a11y.point", {
              part: t(PARTS[part].name),
              step: t(`journal.a11y.step.${strengthStep(point)}`),
            })
          : ""}
      </span>
    </div>
  );
}
