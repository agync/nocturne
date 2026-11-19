import type { EntityKind } from "../../engine/bindings";

const PATHS: Record<EntityKind, React.ReactNode> = {
  person: (
    <>
      <circle cx="12" cy="8" r="3.5" />
      <path d="M5 20c0-3.9 3.1-6 7-6s7 2.1 7 6" />
    </>
  ),
  place: (
    <>
      <path d="M12 21s-6.5-5.6-6.5-11a6.5 6.5 0 0 1 13 0c0 5.4-6.5 11-6.5 11z" />
      <circle cx="12" cy="10" r="2.3" />
    </>
  ),
  thing: <path d="M12 3.5l8.5 8.5-8.5 8.5L3.5 12z" />,
};

/** A person, a place or a thing, by its shape. Decorative: the name sits beside it. */
export function NameMark({ kind, size = 14 }: { kind: EntityKind; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[kind]}
    </svg>
  );
}
