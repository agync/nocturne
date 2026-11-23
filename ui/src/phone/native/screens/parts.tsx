import { invoke } from "@tauri-apps/api/core";
import { Children, type ReactNode, useEffect, useRef } from "react";
import { formatTimeOfDay } from "../../../copy/format";
import { Button } from "../../screens/ui";
import { type CopyId, type CopyValues, t, tParts } from "../../../copy/t";

/** The privacy policy, in the phone's browser. */
export const POLICY = "https://nocturne.is/privacy";

export function openPolicy(): void {
  void invoke("plugin:opener|open_url", { url: POLICY }).catch(() => undefined);
}

/** A sentence that names a place in the app, as plain words. */
export function places(id: CopyId, values?: CopyValues): string {
  return t(id, {
    thisPhone: t("phone.sources.thisPhone"),
    fromYourPhones: t("phone.sources.fromYourPhones"),
    ...values,
  });
}

/** The same, with each place in bold, as the design writes them. */
export function placesRich(id: CopyId): ReactNode {
  return Children.toArray(
    tParts(id, {
      thisPhone: <strong>{t("phone.sources.thisPhone")}</strong>,
      fromYourPhones: <strong>{t("phone.sources.fromYourPhones")}</strong>,
      deleteReadings: <strong>{t("phone.alone.sources.delete")}</strong>,
    }) as ReactNode[],
  );
}

/** The heading takes focus each time the step changes, so a screen reader starts there. */
export function useHeading(step: string) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    void step;
    heading.current?.focus();
  }, [step]);
  return heading;
}

export function Switch({
  label,
  hint,
  sub,
  on,
  onChange,
  dot,
  disabled,
}: {
  label: string;
  hint?: string;
  /** A second line that tells apart rows with the same label, such as a calendar's account. */
  sub?: string;
  on: boolean;
  onChange(next: boolean): void;
  /** A calendar's colour, as the phone shows it. */
  dot?: string | null;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-disabled={disabled ? true : undefined}
      className="ph-switch-row ph-bring-row"
      onClick={disabled ? undefined : () => onChange(!on)}
    >
      {dot !== undefined && (
        <span
          className="ph-bring-dot"
          aria-hidden="true"
          style={dot ? { background: dot } : undefined}
        />
      )}
      <span className="ph-switch-label">
        <span className="ph-bring-label">{label}</span>
        {(hint ?? sub) && <span className="ph-hint ph-bring-hint">{hint ?? sub}</span>}
      </span>
      <span className="ph-switch" aria-hidden="true" />
    </button>
  );
}

/** "Done", pinned at the bottom of a settings screen, where the thumb is. Choices save as made. */
export function DoneBar({ onDone }: { onDone(): void }) {
  return (
    <footer className="ph-done">
      <Button kind="primary" onClick={onDone}>
        {t("phone.done")}
      </Button>
    </footer>
  );
}

/** A phone alone: what was kept stays until deleted, and the way to delete it. */
export function KeptSoFar({ onDelete }: { onDelete?(): void }) {
  return (
    <section className="ph-kept-so-far" aria-labelledby="ph-kept-so-far">
      <h2 className="ph-lbl" id="ph-kept-so-far">
        {t("phone.alone.sources.kept")}
      </h2>
      <p className="ph-hint">{t("phone.alone.sources.already")}</p>
      {onDelete ? (
        <button type="button" className="ph-link ph-link--careful" onClick={onDelete}>
          {t("phone.alone.sources.delete")}
        </button>
      ) : null}
    </section>
  );
}

/** "7:42 am", or "Tuesday 7:42 am" for an earlier day. */
export function sentAt(iso: string, now = new Date()): string {
  const at = new Date(iso);
  const time = formatTimeOfDay(at);
  if (at.toDateString() === now.toDateString()) return time;
  const day = new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(at);
  return `${day} ${time}`;
}
