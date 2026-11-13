import type { ReactNode } from "react";

/** Page text marks emphasis with **double asterisks**. Nothing else is interpreted. */
export function richText(text: string): ReactNode[] {
  return text.split("**").map((part, index) => {
    const key = `${index}-${part}`;
    return index % 2 === 1 ? <strong key={key}>{part}</strong> : part;
  });
}

/** The same text with the markers removed, for places that cannot show emphasis. */
export function plainText(text: string): string {
  return text.split("**").join("");
}
