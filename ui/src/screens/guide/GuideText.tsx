import type { GuideInline } from "../../engine/bindings";
import { ControlName } from "./ControlName";

/** A run of guide text: its stressed words, controls and file names kept apart. */
export function GuideText({ parts }: { parts: GuideInline[] }) {
  return (
    <>
      {parts.map((part, index) => {
        const key = `${index}-${part.text}`;
        switch (part.type) {
          case "text":
            return <span key={key}>{part.text}</span>;
          case "strong":
            return <strong key={key}>{part.text}</strong>;
          case "control":
            return <ControlName key={key}>{part.text}</ControlName>;
          default:
            return <code key={key}>{part.text}</code>;
        }
      })}
    </>
  );
}
