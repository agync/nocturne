import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "../../components/Button/Button";
import { formatDuration } from "../../copy/format";
import { t } from "../../copy/t";
import type { PairingRequest } from "../../engine/bindings";
import styles from "./Away.module.css";

const FOCUSABLE = "button:not([disabled]), [href], [tabindex]:not([tabindex='-1'])";

interface PairingDialogProps {
  request: PairingRequest;
  onDecide: (allow: boolean) => void;
  busy: boolean;
}

/**
 * A phone asking to be added. It takes focus when it opens, reads the three words out one by one,
 * and only ever changes anything when the person chooses. Escape means "Don't allow".
 */
export function PairingDialog({ request, onDecide, busy }: PairingDialogProps) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const describedId = useId();
  const [left, setLeft] = useState(() =>
    Math.max(0, new Date(request.decideBy).getTime() - Date.now()),
  );
  const decide = useRef(onDecide);
  decide.current = onDecide;
  const working = useRef(busy);
  working.current = busy;

  useEffect(() => {
    const target = new Date(request.decideBy).getTime();
    const timer = setInterval(() => setLeft(Math.max(0, target - Date.now())), 1000);
    return () => clearInterval(timer);
  }, [request.decideBy]);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const root = document.getElementById("root");
    root?.setAttribute("inert", "");
    panel.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        // A decision already on its way is not changed by a second one.
        if (!working.current) decide.current(false);
        return;
      }
      if (event.key !== "Tab" || !panel.current) return;
      const focusable = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) return;
      const active = document.activeElement;
      if (event.shiftKey && (active === first || active === panel.current)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      root?.removeAttribute("inert");
      opener?.focus?.();
    };
  }, []);

  return createPortal(
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.pairing}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={describedId}
        tabIndex={-1}
      >
        <h2 id={titleId} className={styles.statement}>
          {t("away.pair.title")}
        </h2>
        <div id={describedId} className={styles.pairBody}>
          <p>
            {t("away.pair.calls")} <strong>{request.deviceName}</strong>
            {t("away.pair.check")}
          </p>
          <ol className={styles.words} aria-label={t("away.pair.words")}>
            {request.checkPhrase.map((word, i) => (
              <li key={`${i}-${word}`}>{word}</li>
            ))}
          </ol>
          <p className={styles.muted}>{t("away.pair.warning")}</p>
        </div>
        <div className={styles.pairFoot}>
          <span className={styles.muted} role="timer" aria-live="off">
            {t("away.pair.refusedIn", { time: formatDuration(left) })}
          </span>
          <span className={styles.buttonsInline}>
            <Button variant="secondary" disabled={busy} onClick={() => onDecide(false)}>
              {t("away.pair.deny")}
            </Button>
            <Button variant="primary" disabled={busy} onClick={() => onDecide(true)}>
              {t("away.pair.allow")}
            </Button>
          </span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
