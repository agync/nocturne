import { type FormEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { formatLongDay, formatTimeOfDay, journalDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { FoundItem, ItemKind, Receipt } from "../../../engine/bindings";
import { Icon, type IconName, Page } from "../../screens/ui";
import type { MindStatus } from "../mind";
import { MindNotice, UnreachableAsk } from "./MindNotice";

type State =
  | { name: "idle" }
  | { name: "finding"; query: string }
  | { name: "found"; query: string; items: FoundItem[] }
  | { name: "failed"; query: string };

const KIND_ICON: Partial<Record<ItemKind, IconName>> = {
  text: "page",
  audio: "mic",
  image: "image",
  document: "document",
  web: "page",
  calendar: "calendar",
};

function kindWord(kind: ItemKind): string {
  switch (kind) {
    case "audio":
      return t("kind.audio");
    case "image":
      return t("kind.image");
    case "document":
      return t("kind.document");
    case "web":
      return t("kind.web");
    case "calendar":
      return t("kind.calendar");
    default:
      return t("kind.text");
  }
}

/** "Today 7:05 am", or "Thursday 19 November" for an earlier day. */
function when(receipt: Receipt, today: string): string {
  return journalDay(receipt.keptAt) === today
    ? `${t("phone.app.tab.today")} ${formatTimeOfDay(receipt.keptAt)}`
    : formatLongDay(receipt.keptAt);
}

function Found({
  item,
  today,
  onOpen,
}: {
  item: FoundItem;
  today: string;
  onOpen(r: Receipt, words: string | null): void;
}) {
  const r = item.receipt;
  const icon = KIND_ICON[r.kind];
  const words = item.excerpt.map((part) => part.text).join("");
  const day = when(r, today);
  return (
    <li>
      <button
        type="button"
        className="ph-found-row"
        onClick={() => onOpen(r, item.excerpt.find((p) => p.matched)?.text ?? null)}
      >
        <span className="sr-only">
          {t("phone.alone.find.result", { words, day, kind: kindWord(r.kind) })}
        </span>
        <span aria-hidden="true">
          {item.excerpt.map((part, i) =>
            part.matched ? <mark key={String(i)}>{part.text}</mark> : part.text,
          )}
        </span>
        <span className="ph-found-meta" aria-hidden="true">
          {day}
          <span className="ph-found-chip">
            {icon ? <Icon name={icon} size={12} /> : null}
            {r.kind === "calendar" && r.calendar
              ? t("phone.app.receipt.calendar", { calendar: r.calendar })
              : kindWord(r.kind)}
          </span>
          {item.private ? <Icon name="lock" size={12} /> : null}
        </span>
      </button>
    </li>
  );
}

/**
 * Ask on a phone alone: what was kept, found by its words, newest first among equal matches,
 * each with its receipt. Answers need a mind, or one that is answering: while the chosen
 * service is not, `status` says why.
 */
export function HomeFind({
  bar,
  empty,
  today,
  find,
  onOpen,
  onMind,
  mind = false,
  status = null,
  onFixMind,
}: {
  bar: ReactNode;
  /** Nothing has been kept yet. */
  empty: boolean;
  today: string;
  find(query: string): Promise<FoundItem[]>;
  /** Opens what was found, with the first matched words to mark there. */
  onOpen(receipt: Receipt, words: string | null): void;
  onMind(): void;
  /** A mind is chosen: nothing says one is needed. */
  mind?: boolean;
  /** How the chosen service is doing. */
  status?: MindStatus | null;
  /** "Check the key…", "Check again" and "Change limit". */
  onFixMind?(): void;
}) {
  const [draft, setDraft] = useState("");
  const [state, setState] = useState<State>({ name: "idle" });
  const field = useRef<HTMLInputElement>(null);
  const asked = useRef(0);

  useEffect(() => {
    const query = draft.trim();
    const n = ++asked.current;
    if (query === "") return setState({ name: "idle" });
    const timer = setTimeout(() => {
      setState({ name: "finding", query });
      find(query).then(
        (items) => n === asked.current && setState({ name: "found", query, items }),
        () => n === asked.current && setState({ name: "failed", query }),
      );
    }, 250);
    return () => clearTimeout(timer);
  }, [draft, find]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    field.current?.blur();
  };

  return (
    <Page bar={bar} className="ph-tabbed">
      <main className="ph-main ph-main--tight" style={{ gap: 16 }}>
        {status && onFixMind ? <MindNotice status={status} onFix={onFixMind} /> : null}
        {status?.state === "unreachable" ? <UnreachableAsk host={status.host} /> : null}
        <form onSubmit={submit}>
          <div className="ph-find">
            <input
              ref={field}
              type="text"
              aria-label={t("phone.alone.find.placeholder")}
              placeholder={t("phone.alone.find.placeholder")}
              value={draft}
              autoComplete="off"
              enterKeyHint="search"
              onChange={(e) => setDraft(e.target.value)}
            />
            {draft === "" ? (
              <span className="ph-ib" aria-hidden="true">
                <Icon name="ask" size={18} />
              </span>
            ) : (
              <button
                type="button"
                className="ph-ib ph-ib--ink"
                aria-label={t("phone.alone.find.clear")}
                onClick={() => {
                  setDraft("");
                  field.current?.focus();
                }}
              >
                <Icon name="close" size={18} />
              </button>
            )}
          </div>
        </form>
        {state.name === "idle" ? (
          <div className="ph-find-empty">
            <Icon name="ask" size={32} />
            <p className="ph-body ph-ink2">
              {empty ? t("phone.alone.find.empty") : t("phone.alone.find.before")}
            </p>
          </div>
        ) : state.name === "failed" ? (
          <p className="ph-small" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : (
          <>
            <p className="ph-small" role="status">
              {state.name === "found" && state.items.length === 0
                ? t("phone.alone.find.none")
                : mind
                  ? t("phone.mind.find.above")
                  : t("phone.alone.find.above")}
            </p>
            {state.name === "found" && state.items.length > 0 ? (
              <ul className="ph-kept-list">
                {state.items.map((item) => (
                  <Found key={item.receipt.itemId} item={item} today={today} onOpen={onOpen} />
                ))}
              </ul>
            ) : null}
            {mind ? null : (
              <button type="button" className="ph-link" onClick={onMind}>
                {t("phone.alone.noMind.choose")}
              </button>
            )}
          </>
        )}
      </main>
    </Page>
  );
}
