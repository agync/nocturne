import { type ReactNode, type RefObject, useEffect, useRef } from "react";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Keeps Tab inside `box` while it is mounted, puts focus on its first control, and hands focus
 * back to where it was on leaving.
 */
export function useFocusTrap(box: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = box.current;
    if (!node) return;
    const before = document.activeElement as HTMLElement | null;
    const controls = () =>
      [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
        (el) => el.getAttribute("aria-hidden") !== "true",
      );
    (controls()[0] ?? node).focus({ preventScroll: true });
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const all = controls();
      const first = all[0];
      const last = all[all.length - 1];
      if (!first || !last) return;
      const active = document.activeElement;
      if (e.shiftKey && (active === first || !node.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !node.contains(active))) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      if (before?.isConnected) before.focus?.({ preventScroll: true });
    };
  }, [box]);
}

/**
 * While `open`, the phone's own back gesture closes it: an entry is pushed on opening and taken
 * off again on closing by hand. The entry keeps the screen's view, so history holds no content.
 */
export function useBackToClose(open: boolean, onClose: () => void) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!open) return;
    let entered = true;
    history.pushState({ view: history.state?.view, sheet: true }, "");
    const onPop = () => {
      entered = false;
      close.current();
    };
    window.addEventListener("popstate", onPop);
    return () => {
      window.removeEventListener("popstate", onPop);
      if (entered && history.state?.sheet) history.back();
    };
  }, [open]);
}

interface SheetProps {
  label: string;
  onClose(): void;
  children: ReactNode;
  className?: string;
}

/** A sheet from the bottom edge: a dialog that traps focus and closes on Escape and back. */
export function BottomSheet({ label, onClose, children, className }: SheetProps) {
  const box = useRef<HTMLElement>(null);
  useFocusTrap(box);
  useBackToClose(true, onClose);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <section
      ref={box}
      className={`ph-sheet ph-sheet--solid${className ? ` ${className}` : ""}`}
      role="dialog"
      aria-modal="true"
      aria-label={label}
      tabIndex={-1}
    >
      <span className="ph-grip" aria-hidden="true" />
      {children}
    </section>
  );
}

/** Runs `next` once the sheet's own history entry is gone, so a move leaves no stale entry behind. */
export function afterSheet(next: () => void) {
  if (!history.state?.sheet) return next();
  const run = () => {
    window.removeEventListener("popstate", run);
    next();
  };
  window.addEventListener("popstate", run);
  history.back();
}
