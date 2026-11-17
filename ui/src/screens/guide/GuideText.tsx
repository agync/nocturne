import { ControlChip } from "../../components/ControlChip/ControlChip";
import type { ChipId } from "../../components/ControlChip/registry";
import type { GuideInline } from "../../engine/bindings";

/** A run of guide text: its stressed words, control chips and file names kept apart. */
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
            return <ControlChip key={key} id={part.chip as ChipId} />;
          default:
            return <code key={key}>{part.text}</code>;
        }
      })}
    </>
  );
}
