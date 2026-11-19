import { useEffect, useRef, useState } from "react";
import { Icon } from "../../components/Icon/Icon";
import { choiceText } from "../../components/PageSheet/journalQuestion";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { CheckIn, DayPart, JournalDay, Mention, TimelineRow } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import { FeelingMark } from "../../journal/FeelingMark";
import { PARTS } from "../../journal/feelings";
import { MentionChip } from "../../journal/MentionChip";
import { usePlatform } from "../../platform/context";
import { Glyph, type GlyphName } from "./glyphs";
import styles from "./Timeline.module.css";
import { clock12, weekdayOf } from "./time";

const PARTS_OF_DAY: DayPart[] = ["morning", "afternoon", "evening", "afterMidnight"];

const GROUP = {
  morning: "journal.timeline.group.morning",
  afternoon: "journal.timeline.group.afternoon",
  evening: "journal.timeline.group.evening",
  afterMidnight: "journal.timeline.group.afterMidnight",
} as const satisfies Record<DayPart, Parameters<typeof t>[0]>;

interface TimelineProps {
  data: JournalDay;
  notReady: boolean;
  failed: boolean;
  onRetry: () => void;
  openCard: (mention: Mention, anchor: HTMLElement) => void;
  cardMentionId: string | null;
  onHighlight: (entityId: string | null) => void;
}

const words = (c: CheckIn) => c.feeling.words.join(", ");
const unread = (row: TimelineRow) => row.item.status === "kept" || row.item.status === "reading";

/** What a row says: the words, the voice note's words, or what was seen in a photo. */
function lineOf(row: TimelineRow): string {
  switch (row.kind) {
    case "checkin":
      return row.checkin?.line ? `“${row.checkin.line}”` : "";
    case "good":
      return row.good?.text ?? row.item.title;
    case "photo":
      return row.item.summary ?? (unread(row) ? t("kind.image") : row.item.title);
    case "voice":
      return row.item.text ?? t("kind.audio");
    case "elsewhere":
      return `${row.item.title} · ${t(`kind.${row.item.kind}`)}`;
    case "entry":
      return "";
    default:
      return row.item.text ?? row.item.title;
  }
}

function glyphOf(row: TimelineRow): GlyphName {
  if (row.kind === "voice") return "mic";
  if (row.kind === "elsewhere") {
    if (row.item.kind === "web") return "web";
    if (row.item.kind === "image") return "image";
    return "file";
  }
  return row.item.fromDevice && row.kind === "note" ? "phone" : "pen";
}

const ITEMS = '[role="menuitem"], [role="menuitemcheckbox"]';

/** A row's small menu: change, leave out, move, and each name it mentions. */
function RowMenu({
  row,
  mentions,
  link,
  open,
  setOpen,
  openCard,
}: {
  row: TimelineRow;
  mentions: Mention[];
  link: React.RefObject<HTMLElement | null>;
  open: boolean;
  setOpen: (open: boolean) => void;
  openCard: TimelineProps["openCard"];
}) {
  const engine = useEngine();
  const refresh = useRefresh();
  const toast = useToast();
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>(ITEMS)?.focus();
    const away = (event: PointerEvent) => {
      const target = event.target as Node;
      if (menu.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [open, setOpen]);

  const close = (focus: boolean) => {
    setOpen(false);
    if (focus) link.current?.focus();
  };

  const leaveOut = async () => {
    close(true);
    const next = !row.leftOut;
    try {
      await engine.setLeftOut(row.item.id, next);
      refresh("journal");
      if (!next) return;
      toast.show({
        message: t("journal.timeline.leftOut"),
        action: {
          label: t("journal.timeline.undo"),
          onAction: () => {
            engine.setLeftOut(row.item.id, false).then(
              () => refresh("journal"),
              () => toast.show({ message: t("memories.failed") }),
            );
          },
        },
      });
    } catch {
      toast.show({ message: t("memories.failed") });
    }
  };

  const move = async () => {
    close(true);
    try {
      await engine.moveToDay(row.item.id, row.moveTo as string);
      refresh("journal");
    } catch {
      toast.show({ message: t("memories.failed") });
    }
  };

  if (!open) return null;
  return (
    <div
      ref={menu}
      className={styles.menu}
      role="menu"
      aria-label={t("journal.timeline.rowMenu")}
      onKeyDown={(event) => {
        const items = [...(menu.current?.querySelectorAll<HTMLElement>(ITEMS) ?? [])];
        const at = items.indexOf(document.activeElement as HTMLElement);
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close(true);
        } else if (event.key === "ArrowDown") {
          event.preventDefault();
          items[(at + 1) % items.length]?.focus();
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          items[(at - 1 + items.length) % items.length]?.focus();
        } else if (event.key === "Tab") {
          setOpen(false);
        }
      }}
    >
      {row.kind === "checkin" ? (
        <a
          role="menuitem"
          className={styles.menuItem}
          href={`#/journal/check-in?change=${row.item.id}`}
        >
          {t("journal.timeline.change")}
        </a>
      ) : null}
      <button
        type="button"
        role="menuitemcheckbox"
        aria-checked={row.leftOut}
        className={styles.menuItem}
        onClick={() => void leaveOut()}
      >
        {t("journal.timeline.leaveOut")}
      </button>
      {row.moveTo ? (
        <button
          type="button"
          role="menuitem"
          className={styles.menuItem}
          onClick={() => void move()}
        >
          {t("journal.timeline.moveTo", { weekday: weekdayOf(row.moveTo) })}
        </button>
      ) : null}
      {mentions.map((m) => (
        <button
          key={m.id}
          type="button"
          role="menuitem"
          aria-haspopup="dialog"
          className={styles.menuItem}
          onClick={() => {
            setOpen(false);
            if (link.current) openCard(m, link.current);
          }}
        >
          {m.name}
        </button>
      ))}
    </div>
  );
}

function Row({
  row,
  data,
  openCard,
  cardMentionId,
  onHighlight,
}: {
  row: TimelineRow;
  data: JournalDay;
  openCard: TimelineProps["openCard"];
  cardMentionId: string | null;
  onHighlight: TimelineProps["onHighlight"];
}) {
  const platform = usePlatform();
  const link = useRef<HTMLAnchorElement | HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const tone = (row.checkin ?? row.feeling)?.feeling.part;
  const time = clock12(row.item.keptAt);
  const timeText =
    row.part === "afterMidnight" ? t("journal.timeline.afterMidnight", { time }) : time;
  const mentions = row.mentions.filter((m) => m.state !== "removed");
  const text = lineOf(row);
  const reading =
    unread(row) && row.kind !== "entry" && row.kind !== "checkin" && row.kind !== "answer";
  const hasMenu = row.kind !== "entry";
  const photo = row.kind === "photo";
  const src = row.item.originalPath ? platform.originalSrc(row.item.originalPath) : null;

  const feel = row.checkin ? (
    <span className={styles.feel} data-part={tone}>
      <FeelingMark mark={row.checkin.feeling.part} size={14} />
      {`${t(PARTS[row.checkin.feeling.part].name)} · ${words(row.checkin)}`}
    </span>
  ) : null;
  const tapped = row.feeling ? (
    <span className={`${styles.feel} ${styles.right}`} data-part={tone}>
      <FeelingMark mark={row.feeling.feeling.part} size={14} />
      {words(row.feeling)}
    </span>
  ) : null;

  const lead =
    row.kind === "good" ? (
      <FeelingMark mark="sun" size={16} tone={false} />
    ) : photo ? (
      src ? (
        <img className={styles.thumb} src={src} alt="" />
      ) : (
        <span className={styles.thumb} aria-hidden="true">
          <Glyph name="image" />
        </span>
      )
    ) : row.kind === "checkin" ? null : (
      <Glyph name={glyphOf(row)} />
    );

  const body = (
    <>
      <span className={styles.time}>{timeText}</span>
      <span className={styles.line}>
        {lead}
        {feel}
        {row.kind === "entry" ? (
          <span className={styles.text}>
            {data.today
              ? t("journal.entry.heading.today")
              : t("journal.entry.heading.day", { weekday: weekdayOf(data.day) })}
          </span>
        ) : row.answer ? (
          <span className={styles.text}>
            <span className={styles.choice}>{choiceText(row.answer.choice)}</span>
            {row.answer.line ? ` ${row.answer.line}` : ""}
          </span>
        ) : (
          <span className={styles.text}>{text}</span>
        )}
        {reading ? <span className={styles.status}>{t("item.status.reading")}</span> : null}
        {row.leftOut ? (
          <span className={styles.leftOut}>
            <Icon name="eye-off" size={14} label={t("journal.timeline.leaveOut")} />
          </span>
        ) : null}
        {tapped}
      </span>
    </>
  );

  const entryTarget = () => {
    const heading = document.getElementById("entry-heading");
    heading?.scrollIntoView({ block: "start" });
    document.querySelector<HTMLTextAreaElement>("textarea")?.focus();
  };

  return (
    <li className={styles.row} data-part={tone} data-kind={row.kind} data-left-out={row.leftOut}>
      {row.kind === "entry" ? (
        <button
          type="button"
          ref={link as React.RefObject<HTMLButtonElement>}
          className={styles.link}
          onClick={entryTarget}
        >
          {body}
        </button>
      ) : (
        <a
          ref={link as React.RefObject<HTMLAnchorElement>}
          className={styles.link}
          href={`#/items/${row.item.id}`}
          aria-haspopup={hasMenu ? "menu" : undefined}
          aria-keyshortcuts={hasMenu ? "Shift+F10" : undefined}
          onKeyDown={(event) => {
            if (!hasMenu) return;
            if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
              event.preventDefault();
              setMenuOpen(true);
            }
          }}
          onContextMenu={(event) => {
            if (!hasMenu) return;
            event.preventDefault();
            setMenuOpen(true);
          }}
        >
          {body}
        </a>
      )}
      {row.kind === "checkin" ? (
        <a
          className={styles.change}
          tabIndex={-1}
          href={`#/journal/check-in?change=${row.item.id}`}
        >
          {t("journal.timeline.change")}
        </a>
      ) : null}
      {hasMenu ? (
        <button
          type="button"
          className={styles.more}
          tabIndex={-1}
          aria-label={t("journal.timeline.rowMenu")}
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen(!menuOpen)}
        >
          <Glyph name="more" />
        </button>
      ) : null}
      {hasMenu ? (
        <RowMenu
          row={row}
          mentions={mentions}
          link={link}
          open={menuOpen}
          setOpen={setMenuOpen}
          openCard={openCard}
        />
      ) : null}
      {row.item.fromDevice || mentions.length > 0 || row.answer ? (
        <div className={styles.sub}>
          {row.answer ? (
            <a className={styles.about} href={`#/items/${row.answer.about}`} tabIndex={-1}>
              {t("journal.timeline.about", { sentence: row.answer.sentence })}
            </a>
          ) : null}
          {row.item.fromDevice ? (
            <span className={styles.meta}>
              {t("journal.timeline.fromDevice", { phone: row.item.fromDevice })}
            </span>
          ) : null}
          {mentions.map((m) => (
            <MentionChip
              key={m.id}
              name={m.name}
              kind={m.kind}
              isNew={m.new}
              tabIndex={-1}
              pressed={cardMentionId === m.id}
              onClick={(event) => openCard(m, event.currentTarget)}
              onMouseEnter={() => onHighlight(m.entityId)}
              onMouseLeave={() => onHighlight(null)}
            />
          ))}
        </div>
      ) : null}
    </li>
  );
}

/** "Through the day": everything kept that day, oldest first, in four parts of the day. */
export function Timeline({
  data,
  notReady,
  failed,
  onRetry,
  openCard,
  cardMentionId,
  onHighlight,
}: TimelineProps) {
  const empty = data.rows.length === 0;
  return (
    <section className={styles.timeline} aria-labelledby="timeline-heading">
      <h2 className={styles.heading} id="timeline-heading">
        {t("journal.timeline.heading")}
      </h2>
      {failed ? (
        <div className={styles.problem} role="alert">
          <span>{t("journal.timeline.error")}</span>
          <button type="button" className={styles.retry} onClick={onRetry}>
            {t("button.tryAgain")}
          </button>
        </div>
      ) : null}
      {notReady ? (
        <div className={styles.notice} role="status">
          <strong>{t("journal.notReady.title")}</strong>
          <span>{t("journal.notReady.body")}</span>
        </div>
      ) : null}
      {empty && !failed ? (
        <p className={styles.empty}>
          {t(data.today ? "journal.timeline.empty" : "journal.timeline.emptyPast")}
        </p>
      ) : null}
      {PARTS_OF_DAY.map((part) => {
        const rows = data.rows.filter((r) => r.part === part);
        if (rows.length === 0) return null;
        return (
          <section key={part} className={styles.group} aria-labelledby={`group-${part}`}>
            <h3 className={styles.groupLabel} id={`group-${part}`}>
              {t(GROUP[part])}
            </h3>
            <ul className={styles.rows}>
              {rows.map((row) => (
                <Row
                  key={`${row.kind}-${row.item.id}`}
                  row={row}
                  data={data}
                  openCard={openCard}
                  cardMentionId={cardMentionId}
                  onHighlight={onHighlight}
                />
              ))}
            </ul>
          </section>
        );
      })}
    </section>
  );
}
