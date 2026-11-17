import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useLayoutEffect,
  useState,
} from "react";
import { createPortal } from "react-dom";
import styles from "./ActionBar.module.css";
import { enterFlow } from "./flow";

/** `undefined` outside a host, `null` until the host's slot is on screen. */
const Slot = createContext<HTMLElement | null | undefined>(undefined);

/**
 * Where a sheet or card keeps its action bar: after everything else it holds, so a keyboard or
 * screen reader person meets the words first. A bar written anywhere inside is drawn here.
 */
export function ActionBarHost({ children }: { children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  return (
    <Slot.Provider value={slot}>
      {children}
      <div ref={setSlot} className={styles.slot} data-action-slot="" />
    </Slot.Provider>
  );
}

function scrollerOf(element: HTMLElement): HTMLElement {
  for (let node = element.parentElement; node; node = node.parentElement) {
    const overflow = getComputedStyle(node).overflowY;
    if (overflow === "auto" || overflow === "scroll") return node;
  }
  return (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
}

/**
 * Keeps room at the bottom of the scrolling area as tall as a pinned bar, so a focused field
 * never hides behind it, and says whether there is more to read above the bar.
 */
function watch(bar: HTMLElement, host: HTMLElement | null, onMore: (more: boolean) => void) {
  const root = scrollerOf(bar);
  const page = root === document.scrollingElement || root === document.documentElement;
  const target: HTMLElement | Window = page ? window : root;
  const measure = () => {
    const pinned = getComputedStyle(bar).position === "sticky";
    root.style.scrollPaddingBottom = pinned ? `${bar.offsetHeight}px` : "";
    onMore(pinned && root.scrollTop + root.clientHeight < root.scrollHeight - 1);
  };
  measure();
  target.addEventListener("scroll", measure, { passive: true });
  window.addEventListener("resize", measure);
  let observer: ResizeObserver | undefined;
  if (typeof ResizeObserver !== "undefined") {
    observer = new ResizeObserver(measure);
    observer.observe(bar);
    observer.observe(root);
    const sheet = host?.parentElement ?? bar.parentElement;
    if (sheet) observer.observe(sheet);
  }
  return () => {
    target.removeEventListener("scroll", measure);
    window.removeEventListener("resize", measure);
    observer?.disconnect();
    root.style.scrollPaddingBottom = "";
  };
}

interface ActionBarProps {
  /** The buttons. The main one first: it sits on the left, then the quiet ones. */
  children: ReactNode;
  /** The one line beside the buttons: why the main button is off, or what the step promises. */
  line?: ReactNode;
  /** Lets a disabled button point at the line with `aria-describedby`. */
  lineId?: string;
}

/**
 * The buttons of a step, pinned to the bottom of the window on every step of every flow. The
 * last lines above it fade while there is more to read. In a very short window it stops
 * pinning and sits after the words.
 */
export function ActionBar({ children, line, lineId }: ActionBarProps) {
  const slot = useContext(Slot);
  const [bar, setBar] = useState<HTMLDivElement | null>(null);
  const [more, setMore] = useState(false);

  useEffect(() => enterFlow(), []);
  useLayoutEffect(() => {
    if (!bar) return;
    return watch(bar, slot ?? null, setMore);
  }, [bar, slot]);

  if (slot === null) return null;
  const content = (
    <>
      <div className={styles.spacer} aria-hidden="true" />
      <div ref={setBar} className={styles.bar} data-action-bar="" data-more={more}>
        <div className={styles.buttons}>{children}</div>
        {line ? (
          <span id={lineId} className={styles.line}>
            {line}
          </span>
        ) : null}
      </div>
    </>
  );
  return slot ? createPortal(content, slot) : content;
}
