import type { Matcher } from "@testing-library/react";

const flat = (text: string | null) => (text ?? "").replace(/\s+/g, " ").trim();

/**
 * Finds the smallest element whose text, chips and all, is the sentence. A chip is a span in the
 * middle of a sentence, so the plain `getByText("...")` match, which reads one text node, misses it.
 */
export const sentence =
  (text: string): Matcher =>
  (_content, element) => {
    if (!element) return false;
    const wanted = flat(text);
    const holds = (el: Element) => flat(el.textContent).includes(wanted);
    return holds(element) && ![...element.children].some(holds);
  };

/** The words of a sentence's chips, in order. */
export const chipsIn = (element: Element): string[] =>
  [...element.querySelectorAll("[data-chip]")].map((chip) => flat(chip.textContent));
