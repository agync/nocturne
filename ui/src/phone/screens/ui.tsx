import type { ReactNode } from "react";
import { richText } from "../../components/PageLine/richText";
import { type CopyId, type CopyValues, t } from "../../copy/t";

const PATHS = {
  ask: ["M7.9 20A9 9 0 1 0 4 16.1L2 22Z"],
  plus: ["M5 12h14", "M12 5v14"],
  page: [
    "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
    "M14 2v4a2 2 0 0 0 2 2h4",
    "M16 13H8",
    "M16 17H8",
  ],
  lock: ["M7 11V7a5 5 0 0 1 10 0v4"],
  back: ["m15 18-6-6 6-6"],
  forward: ["m9 18 6-6-6-6"],
  check: ["M20 6 9 17l-5-5"],
  camera: [
    "M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z",
    "M12 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  ],
  image: [
    "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
    "m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21",
    "M9 7.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z",
  ],
  mic: [
    "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z",
    "M19 10v2a7 7 0 0 1-14 0v-2",
    "M12 19v3",
  ],
  moon: ["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"],
  power: ["M18.36 6.64a9 9 0 1 1-12.73 0", "M12 2v10"],
  pin: ["M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z", "M12 13a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"],
  unlink: ["M15 9l-6 6", "M9 9l6 6", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z"],
  share: ["M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8", "m16 6-4-4-4 4", "M12 2v13"],
  addBox: [
    "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
    "M8 12h8",
    "M12 8v8",
  ],
  play: ["m6 3 14 9-14 9V3z"],
  pause: ["M6 4h4v16H6z", "M14 4h4v16h-4z"],
  document: [
    "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
    "M14 2v4a2 2 0 0 0 2 2h4",
  ],
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 22 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {name === "lock" && <rect x="3" y="11" width="18" height="11" rx="2" />}
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

/** Copy with **bold** marks, drawn as emphasis. */
export function Rich({ id, values }: { id: CopyId; values?: CopyValues }) {
  return <>{richText(t(id, values))}</>;
}

export function Brand({ heading }: { heading?: boolean }) {
  const Name = heading ? "h1" : "span";
  return (
    <span className="ph-brand">
      <span className="ph-mark" aria-hidden="true" />
      <Name className="ph-brand-name">{t("phone.name")}</Name>
    </span>
  );
}

/** `heading` makes the name the page's heading, for a screen that has no other. */
export function Bar({ onMenu, heading }: { onMenu?: () => void; heading?: boolean }) {
  return (
    <header className="ph-bar">
      <Brand heading={heading} />
      {onMenu && (
        <button type="button" className="ph-ib" onClick={onMenu} aria-haspopup="dialog">
          {t("phone.menu")}
        </button>
      )}
    </header>
  );
}

export function BackBar({
  title,
  onBack,
  note,
  label,
}: {
  title: string;
  onBack: () => void;
  note?: string;
  label?: string;
}) {
  return (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onBack}
        aria-label={label ?? t("phone.back")}
      >
        <Icon name="back" />
      </button>
      <h1 className="ph-bar-title">{title}</h1>
      {note && <span className="ph-rc ph-bar-note">{note}</span>}
    </header>
  );
}

type ButtonProps = {
  children: ReactNode;
  onClick?: () => void;
  kind?: "primary" | "quiet" | "plain" | "danger";
  disabled?: boolean;
  type?: "button" | "submit";
};

/** `disabled` is announced and stops presses without leaving the tab order, as the artboards show. */
export function Button({
  children,
  onClick,
  kind = "plain",
  disabled,
  type = "button",
}: ButtonProps) {
  const cls = ["ph-btn", kind === "plain" ? "" : `ph-btn--${kind}`].filter(Boolean).join(" ");
  return (
    <button
      type={type}
      className={cls}
      aria-disabled={disabled ? true : undefined}
      onClick={disabled ? undefined : onClick}
    >
      {children}
    </button>
  );
}

export function PrivacyLine({ id }: { id: CopyId }) {
  return (
    <div className="ph-privacy-line">
      <Icon name="lock" size={18} />
      <p className="ph-hint ph-hint--ink2">{t(id)}</p>
    </div>
  );
}

/** A screen that has only a bar and a body, as most of the pairing and offline states do. */
export function Page({
  bar,
  children,
  fixed,
  className,
}: {
  bar: ReactNode;
  children: ReactNode;
  fixed?: boolean;
  className?: string;
}) {
  return (
    <div
      className={`ph-screen${fixed ? " ph-screen--fixed" : ""}${className ? ` ${className}` : ""}`}
    >
      {bar}
      {children}
    </div>
  );
}
