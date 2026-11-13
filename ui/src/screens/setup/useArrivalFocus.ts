import { useEffect, useRef } from "react";

/**
 * Each step of setup replaces the one before, taking the button just pressed with it. Focus goes
 * to the new step's heading, so a keyboard or screen reader user lands on what is now showing
 * instead of at the top of the window.
 */
export function useArrivalFocus() {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus({ preventScroll: true });
  }, []);
  return heading;
}
