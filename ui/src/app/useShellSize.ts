import { type RefObject, useEffect, useState } from "react";
import { type ShellSize, shellSize } from "./breakpoints";

function measure(root: HTMLElement | null): ShellSize {
  const width = root?.clientWidth || window.innerWidth;
  const font = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  return shellSize(width, font);
}

/** The shell's size class. It watches the window and the root text size, so 200% text reflows. */
export function useShellSize(root: RefObject<HTMLElement | null>): ShellSize {
  const [size, setSize] = useState<ShellSize>(() => measure(null));

  useEffect(() => {
    const update = () => setSize(measure(root.current));
    update();
    window.addEventListener("resize", update);
    if (typeof ResizeObserver === "undefined") {
      return () => window.removeEventListener("resize", update);
    }
    // A zero-height probe 45 em wide changes size when the root font size does.
    const probe = document.createElement("div");
    probe.style.cssText =
      "position:absolute;visibility:hidden;height:0;width:45em;pointer-events:none";
    document.body.appendChild(probe);
    const observer = new ResizeObserver(update);
    observer.observe(probe);
    if (root.current) observer.observe(root.current);
    return () => {
      window.removeEventListener("resize", update);
      observer.disconnect();
      probe.remove();
    };
  }, [root]);

  return size;
}
