import { type ReactNode, type RefObject, useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";
import { t } from "../../copy/t";
import { Icon } from "../Icon/Icon";
import styles from "./Dialog.module.css";

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** The control that gets focus first. Defaults to the first focusable one. */
  initialFocus?: RefObject<HTMLElement | null>;
  /** Backdrop clicks close it unless this is false. */
  dismissible?: boolean;
  /** Pins the panel to the left edge, for the narrow menu. */
  edge?: "start";
}

/** Open dialogs, innermost last. Only the innermost answers the keyboard; the rest are inert. */
const open: HTMLElement[] = [];
let savedOverflow = "";

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

/**
 * A modal with a hand-rolled focus trap. Tab cycles inside, Escape closes, and focus goes back to
 * whatever opened it. It is open while mounted.
 */
export function Dialog({
  title,
  onClose,
  children,
  initialFocus,
  dismissible = true,
  edge,
}: DialogProps) {
  const panel = useRef<HTMLDivElement>(null);
  const backdrop = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const mine = backdrop.current as HTMLElement;
    const root = document.getElementById("root");
    open.at(-1)?.setAttribute("inert", "");
    if (open.length === 0) {
      savedOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";
    }
    open.push(mine);
    root?.setAttribute("inert", "");

    const first = initialFocus?.current ?? panel.current?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? panel.current)?.focus();

    return () => {
      open.splice(open.indexOf(mine), 1);
      open.at(-1)?.removeAttribute("inert");
      if (open.length === 0) {
        root?.removeAttribute("inert");
        document.body.style.overflow = savedOverflow;
      }
      opener?.focus?.();
    };
  }, [initialFocus]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (open.at(-1) !== backdrop.current) return;
      if (event.key === "Escape") {
        event.stopImmediatePropagation();
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab" || !panel.current) return;
      const focusable = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (focusable.length === 0) {
        event.preventDefault();
        panel.current.focus();
        return;
      }
      const firstEl = focusable[0] as HTMLElement;
      const lastEl = focusable[focusable.length - 1] as HTMLElement;
      const active = document.activeElement;
      if (event.shiftKey && (active === firstEl || active === panel.current)) {
        event.preventDefault();
        lastEl.focus();
      } else if (!event.shiftKey && active === lastEl) {
        event.preventDefault();
        firstEl.focus();
      } else if (!panel.current.contains(active)) {
        event.preventDefault();
        firstEl.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, []);

  return createPortal(
    // biome-ignore lint/a11y/noStaticElementInteractions: a pointer shortcut; Escape and Close serve the keyboard
    <div
      ref={backdrop}
      role="presentation"
      className={styles.backdrop}
      data-edge={edge}
      onClick={(event) => {
        if (dismissible && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <div className={styles.head}>
          <h2 id={titleId} className={styles.title}>
            {title}
          </h2>
          <button type="button" className={styles.close} onClick={onClose}>
            <Icon name="x" size={16} />
            <span>{t("dialog.close")}</span>
          </button>
        </div>
        <div className={styles.body}>{children}</div>
      </div>
    </div>,
    document.body,
  );
}
