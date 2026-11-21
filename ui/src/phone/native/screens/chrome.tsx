import { type ReactNode, useId } from "react";
import { t } from "../../../copy/t";
import { Icon, type IconName } from "../../screens/ui";

export type Tab = "today" | "journal" | "ask";

const TABS: { tab: Tab; icon: IconName; label: () => string }[] = [
  { tab: "today", icon: "page", label: () => t("phone.app.tab.today") },
  { tab: "journal", icon: "book", label: () => t("phone.app.tab.journal") },
  { tab: "ask", icon: "ask", label: () => t("phone.app.tab.ask") },
];

export const tabTitle = (tab: Tab): string => TABS.find((x) => x.tab === tab)?.label() ?? "";

/** The top app bar on each tab: the mark and the tab's name, and This phone at the right. */
export function TabBar({
  tab,
  action,
  onThisPhone,
}: {
  tab: Tab;
  action?: ReactNode;
  onThisPhone(): void;
}) {
  return (
    <header className="ph-bar ph-appbar">
      <span className="ph-brand">
        <span className="ph-mark" aria-hidden="true" />
        <h1 className="ph-appbar-title">{tabTitle(tab)}</h1>
      </span>
      <span className="ph-appbar-actions">
        {action}
        <button
          type="button"
          className="ph-ib ph-ib--ink ph-ib--round"
          aria-label={t("phone.sheet.title")}
          onClick={onThisPhone}
        >
          <Icon name="settings" />
        </button>
      </span>
    </header>
  );
}

/** Today, Journal and Ask at the bottom, the current one in a pill. */
export function NavBar({ tab, onTab }: { tab: Tab; onTab(tab: Tab): void }) {
  return (
    <nav className="ph-nav" aria-label={t("phone.name")}>
      {TABS.map((x) => (
        <button
          key={x.tab}
          type="button"
          className="ph-nav-tab"
          aria-current={x.tab === tab ? "page" : undefined}
          onClick={() => onTab(x.tab)}
        >
          <span className="ph-nav-icon" aria-hidden="true">
            <Icon name={x.icon} size={22} />
          </span>
          {x.label()}
        </button>
      ))}
    </nav>
  );
}

/** The floating Keep button on Today and Journal. */
export function KeepButton({ onKeep }: { onKeep(): void }) {
  return (
    <button type="button" className="ph-fab" onClick={onKeep}>
      <Icon name="plus" size={22} />
      {t("phone.home.keep.title")}
    </button>
  );
}

/** A screen of the app's settings: groups of rows, each group under its own heading. */
export function Group({
  title,
  foot,
  children,
}: {
  title?: string;
  foot?: ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <section className="ph-sgroup" aria-labelledby={title ? id : undefined}>
      {title ? (
        <h2 className="ph-sghead" id={id}>
          {title}
        </h2>
      ) : null}
      <div className="ph-srows">{children}</div>
      {foot ? <p className="ph-sgfoot">{foot}</p> : null}
    </section>
  );
}

/** One row: an icon, its label and hint, a value, and a chevron when it opens more. */
export function Row({
  icon,
  label,
  hint,
  value,
  onPress,
  danger,
}: {
  icon?: IconName;
  label: string;
  hint?: string;
  value?: string;
  onPress?: () => void;
  danger?: boolean;
}) {
  const inner = (
    <>
      {icon ? <Icon name={icon} size={22} /> : null}
      <span className="ph-srow-text">
        <span className={`ph-srow-label${danger ? " ph-srow-label--danger" : ""}`}>{label}</span>
        {hint ? <span className="ph-srow-hint">{hint}</span> : null}
      </span>
      {value ? <span className="ph-srow-value">{value}</span> : null}
      {onPress && !danger ? <Icon name="forward" size={18} /> : null}
    </>
  );
  return onPress ? (
    <button type="button" className="ph-srow" onClick={onPress}>
      {inner}
    </button>
  ) : (
    <div className="ph-srow">{inner}</div>
  );
}
