import { type ReactNode, useLayoutEffect, useRef } from "react";
import { richText } from "../../components/PageLine/richText";
import { type CopyId, type CopyValues, t } from "../../copy/t";
import { followFeet } from "./foot";

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
  book: [
    "M12 7v14",
    "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z",
  ],
  user: ["M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2", "M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"],
  moon: ["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"],
  calendar: [
    "M8 2v4",
    "M16 2v4",
    "M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
    "M3 10h18",
  ],
  heart: [
    "M19 14c1.5-1.5 3-3.2 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.8 0-3 .5-4.5 2-1.5-1.5-2.7-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4 3 5.5l7 7Z",
  ],
  laptop: [
    "M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9",
    "M2.5 16h19l-1.2 2.4a1 1 0 0 1-.9.6H4.6a1 1 0 0 1-.9-.6Z",
  ],
  settings: [
    "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  ],
  walk: [
    "M13 4a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z",
    "m9 20 3-6 3 2v5",
    "m6 12 3-4h4l3 3 3 1",
    "M10 8 8 14",
  ],
  refresh: [
    "M3 12a9 9 0 0 1 15-6.7L21 8",
    "M21 3v5h-5",
    "M21 12a9 9 0 0 1-15 6.7L3 16",
    "M3 21v-5h5",
  ],
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
  phone: ["M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z", "M12 18h.01"],
  info: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z", "M12 16v-4", "M12 8h.01"],
  shield: [
    "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z",
  ],
  close: ["M18 6 6 18", "m6 6 12 12"],
  search: ["M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14z", "m20 20-4-4"],
  list: ["M8 6h13", "M8 12h13", "M8 18h13", "M3 6h.01", "M3 12h.01", "M3 18h.01"],
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
  const screen = useRef<HTMLDivElement>(null);
  // A screen of fixed layout holds its bar in place itself.
  useLayoutEffect(
    () => (fixed || !screen.current ? undefined : followFeet(screen.current)),
    [fixed],
  );
  return (
    <div
      ref={screen}
      className={`ph-screen${fixed ? " ph-screen--fixed" : ""}${className ? ` ${className}` : ""}`}
    >
      {bar}
      {children}
    </div>
  );
}
