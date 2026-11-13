import { useId } from "react";
import { formatLongDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { AskPlace, OnlineView } from "../../engine/bindings";
import { useOnline } from "../../screens/online/useOnline";
import styles from "./AnswerWhere.module.css";
import { PlaceIcon } from "./PlaceIcon";

/** What the "Answer this" choice shows for Online answers as it stands, or nothing when it is off. */
export type WhereState =
  | { kind: "hidden" }
  | { kind: "online"; host: string }
  | { kind: "paused"; note: string; changeLimit: boolean };

export function whereState(view: OnlineView | undefined): WhereState {
  if (!view?.setUp) return { kind: "hidden" };
  const { state } = view;
  if (state.type === "on") return { kind: "online", host: view.host ?? "" };
  if (state.type === "paused") {
    if (state.pause.type === "limit") {
      return {
        kind: "paused",
        note: t("ask.where.paused.limit", { date: formatLongDay(state.pause.until) }),
        changeLimit: true,
      };
    }
    const id =
      state.pause.type === "keyRefused"
        ? "ask.where.paused.keyRefused"
        : "ask.where.paused.noCredit";
    return { kind: "paused", note: t(id), changeLimit: false };
  }
  if (state.type === "problem") {
    return { kind: "paused", note: t("ask.where.paused.problem"), changeLimit: false };
  }
  return { kind: "hidden" };
}

interface Props {
  place: AskPlace;
  onPlace: (place: AskPlace) => void;
  className?: string;
}

/**
 * "Answer this": online or on this computer, for the one question about to be asked. It is in the
 * tab order before the field it belongs to, so it can be set before the question is sent, and it
 * is drawn below the field. Nothing shows until Online answers has been set up.
 */
export function AnswerWhere({ place, onPlace, className }: Props) {
  const { data: view } = useOnline();
  const state = whereState(view);
  const labelId = useId();
  const hintId = useId();
  const name = useId();
  if (state.kind === "hidden") return null;

  const disabled = state.kind === "paused";
  const checked: AskPlace = disabled ? "thisComputer" : place;
  const options: Array<{ value: AskPlace; label: string; icon: "online" | "computer" }> = [
    {
      value: "auto",
      label: disabled ? t("ask.where.paused") : t("ask.where.online", { host: state.host }),
      icon: "online",
    },
    { value: "thisComputer", label: t("ask.where.computer"), icon: "computer" },
  ];

  return (
    <div className={`${styles.where} ${className ?? ""}`}>
      <span id={labelId} className={styles.label}>
        {t("ask.where.label")}
      </span>
      <div
        role="radiogroup"
        aria-labelledby={labelId}
        aria-describedby={hintId}
        className={styles.group}
      >
        {options.map((option) => {
          const on = option.value === checked;
          const off = disabled && option.value === "auto";
          return (
            <label key={option.value} className={styles.option} data-checked={on} data-off={off}>
              <input
                type="radio"
                name={name}
                className="sr-only"
                checked={on}
                disabled={off}
                onChange={() => onPlace(option.value)}
              />
              <PlaceIcon place={option.icon} size={15} />
              {option.label}
            </label>
          );
        })}
      </div>
      <span id={hintId} className={styles.hint}>
        {state.kind === "paused" ? (
          <>
            {state.note}{" "}
            <a href="#/settings/online">
              {t(state.changeLimit ? "ask.where.change" : "ask.where.open")}
            </a>
          </>
        ) : (
          t(checked === "auto" ? "ask.where.hint.online" : "ask.where.hint.computer")
        )}
      </span>
    </div>
  );
}
