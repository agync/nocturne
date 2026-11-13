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

function ToastView({ toast, onDismiss }: { toast: ToastItem; onDismiss: (id: number) => void }) {
  const [paused, setPaused] = useState(false);
  const duration = toast.duration ?? (toast.action ? ACTION_DURATION : DEFAULT_DURATION);
  const returnTo = useRef<HTMLElement | null>(null);

  const finish = () => {
    const back = returnTo.current;
    onDismiss(toast.id);
    if (back?.isConnected) back.focus();
  };
  useEffect(() => {
    if (paused) return;
    const timer = setTimeout(() => onDismiss(toast.id), duration);
    return () => clearTimeout(timer);
  }, [paused, duration, onDismiss, toast.id]);

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
        if (event.key === "Escape") finish();
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
            finish();
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
  const next = useRef(1);

  const dismiss = useCallback((id: number) => {
    setToasts((all) => all.filter((x) => x.id !== id));
  }, []);

  const show = useCallback((options: ToastOptions) => {
    const id = next.current++;
    const key = options.key ?? options.message;
    setToasts((all) => {
      const at = all.findIndex((x) => (x.key ?? x.message) === key);
      if (at === -1) return [...all.slice(-2), { ...options, id }];
      return all.map((x, i) => (i === at ? { ...options, id } : x));
    });
    return id;
  }, []);

  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <section className={styles.region} aria-label={t("toast.region")}>
        <output className={styles.stack} aria-live="polite" aria-atomic="false">
          {toasts.map((toast) => (
            <ToastView key={toast.id} toast={toast} onDismiss={dismiss} />
          ))}
        </output>
      </section>
    </ToastContext.Provider>
  );
}
