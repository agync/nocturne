import { invoke } from "@tauri-apps/api/core";
import { Children, type ReactNode, useEffect, useRef } from "react";
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
  on,
  onChange,
  dot,
  disabled,
}: {
  label: string;
  hint?: string;
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
        {hint && <span className="ph-hint ph-bring-hint">{hint}</span>}
      </span>
      <span className="ph-switch" aria-hidden="true" />
    </button>
  );
}

/** "7:42 am", or "Tuesday 7:42 am" for an earlier day. */
export function sentAt(iso: string, now = new Date()): string {
  const at = new Date(iso);
  const time = new Intl.DateTimeFormat("en-GB", {
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  }).format(at);
  if (at.toDateString() === now.toDateString()) return time;
  const day = new Intl.DateTimeFormat("en-GB", { weekday: "long" }).format(at);
  return `${day} ${time}`;
}
