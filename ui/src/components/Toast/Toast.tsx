import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { t } from "../../copy/t";
import styles from "./Toast.module.css";

export interface ToastOptions {
  /** The short bold start: "Kept." */
  lead?: string;
  message: string;
  /**
   * What makes two toasts the same one. Showing a toast whose key is already on screen replaces
   * it and starts its clock again. Defaults to the message.
   */
  key?: string;
  action?: { label: string; onAction: () => void };
  /**
   * Called once when the toast closes without its action: by timeout, by Escape, by being pushed
   * off the stack or replaced, or by `dismiss`. Use it to commit what the action would undo.
   */
  onExpire?: () => void;
  /** Milliseconds on screen. Default 6000, long enough to press Undo. */
  duration?: number;
}

interface ToastItem extends ToastOptions {
  id: number;
}

interface ToastApi {
  show: (options: ToastOptions) => number;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

export function useToast(): ToastApi {
  const api = useContext(ToastContext);
  if (!api) throw new Error("useToast needs a ToastProvider");
  return api;
}

const DEFAULT_DURATION = 6000;
/** A toast with something to press stays longer, and while the pointer or focus is on it. */
const ACTION_DURATION = 10_000;

type Close = (id: number, expired: boolean) => void;

function ToastView({ toast, onClose }: { toast: ToastItem; onClose: Close }) {
  const [paused, setPaused] = useState(false);
  const duration = toast.duration ?? (toast.action ? ACTION_DURATION : DEFAULT_DURATION);
  const returnTo = useRef<HTMLElement | null>(null);

  const finish = (expired: boolean) => {
    const back = returnTo.current;
    onClose(toast.id, expired);
    if (back?.isConnected) back.focus();
  };
  useEffect(() => {
    if (paused) return;
    const timer = setTimeout(() => onClose(toast.id, true), duration);
    return () => clearTimeout(timer);
  }, [paused, duration, onClose, toast.id]);

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: hover and focus only pause the timer
    <div
      role="presentation"
      className={styles.toast}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      data-region={toast.action ? "toast" : undefined}
      onFocus={(event) => {
        setPaused(true);
        const from = event.relatedTarget;
        if (from instanceof HTMLElement && !event.currentTarget.contains(from))
          returnTo.current = from;
      }}
      onBlur={() => setPaused(false)}
      onKeyDown={(event) => {
        if (event.key === "Escape") finish(true);
      }}
    >
      {toast.lead ? <strong className={styles.lead}>{toast.lead}</strong> : null}
      <span className={styles.message}>{toast.message}</span>
      {toast.action ? (
        <button
          type="button"
          className={styles.action}
          onClick={() => {
            toast.action?.onAction();
            finish(false);
          }}
        >
          {toast.action.label}
        </button>
      ) : null}
    </div>
  );
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const current = useRef<ToastItem[]>([]);
  const next = useRef(1);

  const replaceAll = useCallback((all: ToastItem[]) => {
    current.current = all;
    setToasts(all);
  }, []);

  const close = useCallback<Close>(
    (id, expired) => {
      const gone = current.current.find((x) => x.id === id);
      if (!gone) return;
      replaceAll(current.current.filter((x) => x.id !== id));
      if (expired) gone.onExpire?.();
    },
    [replaceAll],
  );

  const dismiss = useCallback((id: number) => close(id, true), [close]);

  const show = useCallback(
    (options: ToastOptions) => {
      const id = next.current++;
      const key = options.key ?? options.message;
      const all = current.current;
      const at = all.findIndex((x) => (x.key ?? x.message) === key);
      let kept: ToastItem[];
      let dropped: ToastItem[];
      if (at === -1) {
        kept = all.slice(-2);
        dropped = all.slice(0, all.length - kept.length);
        kept = [...kept, { ...options, id }];
      } else {
        kept = all.map((x, i) => (i === at ? { ...options, id } : x));
        dropped = [all[at] as ToastItem];
      }
      replaceAll(kept);
      for (const toast of dropped) toast.onExpire?.();
      return id;
    },
    [replaceAll],
  );

  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <section className={styles.region} aria-label={t("toast.region")}>
        <output className={styles.stack} aria-live="polite" aria-atomic="false">
          {toasts.map((toast) => (
            <ToastView key={toast.id} toast={toast} onClose={close} />
          ))}
        </output>
      </section>
    </ToastContext.Provider>
  );
}
