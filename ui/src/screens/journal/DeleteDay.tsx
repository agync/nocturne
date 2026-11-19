import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import checkbox from "../../components/Checkbox/Checkbox.module.css";
import { Dialog } from "../../components/Dialog/Dialog";
import { Chevron } from "../../components/Disclosure/Chevron";
import { t } from "../../copy/t";
import type {
  ForgetOriginal,
  ForgetPreview,
  ForgetReport,
  JournalDay,
} from "../../engine/bindings";
import { toEngineError } from "../../engine/client";
import { useEngine } from "../../engine/context";
import { FeelingMark } from "../../journal/FeelingMark";
import styles from "./DeleteDay.module.css";
import { cut, type DeleteRow, dayRows, otherRow, rowName } from "./DeleteDayRows";
import { Glyph } from "./glyphs";
import { dayWords } from "./time";

interface DeleteDayProps {
  /** The day as it was when "Delete this day…" was chosen. */
  data: JournalDay;
  onClose: () => void;
  /** Called as soon as the engine has forgotten, even if the dialog has gone by then. */
  onDone: (report: ForgetReport) => void;
}

type Phase = "loading" | "ready" | "working" | "done" | "failedPreview";

/** A token is only good for the choice it was made for, so the two travel together. */
interface Prepared {
  token: string;
  picked: string;
  reflectionLines: number;
  recapLines: number;
}

const isExpired = (error: unknown): boolean => {
  const e = toEngineError(error);
  return /expire/i.test(`${e.code} ${e.detail}`);
};

function Mark({ row }: { row: DeleteRow }) {
  if (row.icon === "sun") return <FeelingMark mark="sun" size={16} tone={false} />;
  if (
    row.icon === "woundUp" ||
    row.icon === "litUp" ||
    row.icon === "low" ||
    row.icon === "settled"
  ) {
    return <FeelingMark mark={row.icon} size={16} />;
  }
  return <Glyph name={row.icon} />;
}

function Box({
  checked,
  mixed = false,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  mixed?: boolean;
  disabled: boolean;
  label: string;
  onChange: (next: boolean) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (input.current) input.current.indeterminate = mixed;
  }, [mixed]);
  return (
    <>
      <input
        ref={input}
        type="checkbox"
        className={checkbox.input}
        checked={checked}
        disabled={disabled}
        aria-label={label}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span
        className={`${checkbox.box} ${styles.box}`}
        aria-hidden="true"
        data-state={mixed ? "mixed" : checked ? "on" : "off"}
      />
    </>
  );
}

function RowItem({
  row,
  ticked,
  cannotGo,
  disabled,
  onToggle,
}: {
  row: DeleteRow;
  ticked: boolean;
  cannotGo: boolean;
  disabled: boolean;
  onToggle: (next: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const name = rowName(row);
  const versions = row.changed
    ? t("journal.delete.versions", { time: row.changed.time, count: row.changed.earlier.length })
    : "";
  const split = versions.indexOf(" · ") + 3;
  const lead = versions.slice(0, split);
  const rest = versions.slice(split);
  const body = (
    <>
      <span className={styles.kind} data-part={typeof row.icon === "string" ? row.icon : undefined}>
        <Mark row={row} />
      </span>
      <span className={styles.when}>{row.time}</span>
      <span className={styles.what}>
        {row.tag && !cannotGo ? <span className={styles.meta}>{row.tag}</span> : null}
        {row.feel ? <span className={styles.feel}>{row.feel}</span> : null}
        {row.text ? <span className={styles.snip}>{cut(row.text)}</span> : null}
        {row.tail && !cannotGo ? <span className={styles.meta}>{row.tail}</span> : null}
        {cannotGo ? <span className={styles.meta}>{t("journal.delete.cannotGo")}</span> : null}
      </span>
    </>
  );
  return (
    <li className={styles.row} data-off={!ticked}>
      {cannotGo ? (
        <div className={styles.line} title={name}>
          <span className={styles.noBox} />
          {body}
        </div>
      ) : (
        // biome-ignore lint/a11y/noLabelWithoutControl: Box draws the checkbox inside this label
        <label className={styles.line} title={name}>
          <span className={styles.boxCell}>
            <Box checked={ticked} disabled={disabled} label={name} onChange={onToggle} />
          </span>
          {body}
        </label>
      )}
      {row.changed ? (
        <div className={styles.extra}>
          <span>{lead}</span>
          <button
            type="button"
            className={styles.disclose}
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            <Chevron size={14} />
            {rest}
          </button>
          {open ? (
            <ul className={styles.versions} aria-label={t("journal.delete.versions.label")}>
              {row.changed.earlier.map((v) => (
                <li key={`${v.time}-${v.text}`}>
                  <span className={styles.when}>{v.time}</span>
                  <span>{v.text}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </li>
  );
}

function Group({
  title,
  note,
  rows,
  ticked,
  cannot,
  disabled,
  onToggleGroup,
  onToggleRow,
  subheads,
}: {
  title: string;
  note?: string;
  rows: DeleteRow[];
  ticked: Set<string>;
  cannot: Set<string>;
  disabled: boolean;
  onToggleGroup: (next: boolean) => void;
  onToggleRow: (row: DeleteRow, next: boolean) => void;
  subheads?: boolean;
}) {
  const live = rows.filter((r) => !cannot.has(r.key));
  const count = live.filter((r) => ticked.has(r.key)).length;
  const all = live.length > 0 && count === live.length;
  const sections = subheads
    ? ([
        ["entry", t("journal.delete.group.entry")],
        ["day", t("journal.delete.group.day")],
      ] as const)
    : ([["other", ""]] as const);
  return (
    <section className={styles.group} aria-label={title}>
      <div className={styles.groupHead}>
        {/* biome-ignore lint/a11y/noLabelWithoutControl: Box draws the checkbox inside this label */}
        <label className={styles.groupBox}>
          <span className={styles.boxCell}>
            <Box
              checked={all}
              mixed={count > 0 && !all}
              disabled={disabled || live.length === 0}
              label={title}
              onChange={() => onToggleGroup(!all)}
            />
          </span>
          <span className={styles.groupLabel}>{title}</span>
        </label>
        <span className={styles.count}>
          {t("journal.delete.count", { ticked: count, total: live.length })}
        </span>
      </div>
      {note ? <p className={styles.groupNote}>{note}</p> : null}
      {sections.map(([section, heading]) => {
        const mine = rows.filter((r) => r.section === section);
        if (mine.length === 0) return null;
        return (
          <div key={section}>
            {heading ? <h3 className={styles.sub}>{heading}</h3> : null}
            <ul className={styles.rows}>
              {mine.map((row) => (
                <RowItem
                  key={row.key}
                  row={row}
                  ticked={ticked.has(row.key)}
                  cannotGo={cannot.has(row.key)}
                  disabled={disabled}
                  onToggle={(next) => onToggleRow(row, next)}
                />
              ))}
            </ul>
          </div>
        );
      })}
    </section>
  );
}

/**
 * "Delete this day": everything kept on the day, one row each, with what else goes and what stays.
 * Focus starts on Keep the day, and once deleting has started the dialog cannot close.
 */
export function DeleteDay({ data, onClose, onDone }: DeleteDayProps) {
  const engine = useEngine();
  const keep = useRef<HTMLElement | null>(null);
  const mine = useMemo(() => dayRows(data), [data]);
  const mineIds = useMemo(() => new Set(mine.flatMap((r) => r.ids)), [mine]);
  const [phase, setPhase] = useState<Phase>("loading");
  const [listing, setListing] = useState<ForgetPreview | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [ticked, setTicked] = useState(() => new Set(mine.map((r) => r.key)));
  const [showMemories, setShowMemories] = useState(false);
  const [refreshed, setRefreshed] = useState(false);
  const [failed, setFailed] = useState(false);
  const [report, setReport] = useState<ForgetReport | null>(null);
  const [outcome, setOutcome] = useState({ rows: 0, others: 0 });
  const alive = useRef(true);
  const latest = useRef(0);
  const doneRef = useRef(onDone);
  doneRef.current = onDone;
  /** Every original from another day listed so far, so an unticked one stays on screen. */
  const known = useRef(new Map<string, ForgetOriginal>());
  const [others, setOthers] = useState<DeleteRow[]>([]);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const rowsAll = useMemo(() => [...mine, ...others], [mine, others]);
  const cannot = useMemo(() => {
    const out = new Set<string>();
    if (!listing) return out;
    const stays = new Set(listing.originals.filter((o) => !o.removable).map((o) => o.itemId));
    for (const row of mine) if (row.ids.some((id) => stays.has(id))) out.add(row.key);
    return out;
  }, [listing, mine]);

  const prepare = useCallback(
    async (keys: Set<string>) => {
      const run = ++latest.current;
      const chosen = [...new Set(rowsAll.filter((r) => keys.has(r.key)).flatMap((r) => r.ids))];
      setPhase("loading");
      if (chosen.length === 0) {
        setPrepared(null);
        setPhase("ready");
        return;
      }
      try {
        const found = await Promise.all(
          chosen.map((itemId) =>
            engine.listMemories({ entityId: null, itemId, status: null, query: null, limit: null }),
          ),
        );
        const memoryIds = [...new Set(found.flat().map((m) => m.id))];
        const full = await engine.previewForget(memoryIds, true, { also: chosen, pick: true });
        if (run !== latest.current || !alive.current) return;
        for (const o of full.originals) {
          if (!mineIds.has(o.itemId)) known.current.set(o.itemId, o);
        }
        setOthers([...known.current.values()].map(otherRow));
        setListing(full);
        setPrepared({
          token: full.token,
          picked: [...keys].sort().join("|"),
          reflectionLines: full.reflectionLines ?? 0,
          recapLines: full.recapLines ?? 0,
        });
        setPhase("ready");
      } catch {
        if (run === latest.current && alive.current) setPhase("failedPreview");
      }
    },
    [engine, rowsAll, mineIds],
  );

  const first = useRef(true);
  useEffect(() => {
    if (!first.current) return;
    first.current = false;
    void prepare(ticked);
  }, [prepare, ticked]);

  const change = (next: Set<string>) => {
    setRefreshed(false);
    setTicked(next);
    void prepare(next);
  };

  const toggleRow = (row: DeleteRow, on: boolean) => {
    const next = new Set(ticked);
    if (on) next.add(row.key);
    else next.delete(row.key);
    change(next);
  };
  const toggleGroup = (group: DeleteRow[], on: boolean) => {
    const next = new Set(ticked);
    for (const row of group) {
      if (cannot.has(row.key)) continue;
      if (on) next.add(row.key);
      else next.delete(row.key);
    }
    change(next);
  };

  const forget = async () => {
    if (!prepared || phase !== "ready") return;
    setPhase("working");
    setFailed(false);
    const rows = mine.filter((r) => ticked.has(r.key) && !cannot.has(r.key)).length;
    const others = rowsAll.filter((r) => r.section === "other" && !ticked.has(r.key)).length;
    const chosen = rows + rowsAll.filter((r) => r.section === "other" && ticked.has(r.key)).length;
    try {
      const result = await engine.forget(prepared.token);
      doneRef.current(result);
      if (!alive.current) return;
      setReport(result);
      setOutcome({ rows: chosen, others });
      setPhase("done");
    } catch (error) {
      if (!alive.current) return;
      if (isExpired(error)) setRefreshed(true);
      else setFailed(true);
      await prepare(ticked);
    }
  };

  const tickedRows = rowsAll.filter((r) => ticked.has(r.key) && !cannot.has(r.key));
  const count = tickedRows.length;
  const allOfDay = cannot.size === 0 && mine.length > 0 && mine.every((r) => ticked.has(r.key));
  const synced = prepared?.picked === [...ticked].sort().join("|");
  const idle = phase === "ready";
  const memories = count > 0 && synced ? (listing?.memories ?? []) : [];
  const lines = count > 0 && synced ? (prepared?.reflectionLines ?? 0) : 0;
  const recap = count > 0 && synced ? (prepared?.recapLines ?? 0) : 0;
  const words = dayWords(data.day);

  if (phase === "done" && report) {
    return (
      <Dialog title={t("journal.delete.done")} initialFocus={keep} onClose={onClose}>
        <div role="status" className={styles.done}>
          <p className={styles.doneText}>
            {report.memoriesRemoved > 0
              ? t("journal.delete.done.memories", {
                  count: outcome.rows,
                  memories: report.memoriesRemoved,
                })
              : t("journal.delete.done.plain", { count: outcome.rows })}
          </p>
          {outcome.others > 0 ? (
            <p className={styles.doneText}>
              {t("journal.delete.done.others", { count: outcome.others })}
            </p>
          ) : null}
        </div>
        <div className={styles.buttons}>
          <Button variant="primary" autoFocus onClick={onClose}>
            {t("button.ok")}
          </Button>
        </div>
      </Dialog>
    );
  }

  const reflection =
    count > 0 && allOfDay && data.reflection !== null
      ? recap > 0
        ? t("journal.delete.reflectionRecap", { count: recap })
        : t("journal.delete.reflection")
      : lines > 0
        ? t("journal.delete.lines", { count: lines })
        : null;

  return (
    <Dialog
      title={t("journal.delete.title", {
        weekday: words.weekday,
        day: words.day,
        month: words.month,
      })}
      initialFocus={keep}
      locked={phase === "working"}
      onClose={onClose}
    >
      <div className={styles.root}>
        <p className={styles.lead}>{t("journal.delete.lead")}</p>
        <div className={styles.scroll}>
          {phase === "loading" && !listing ? (
            <p className={styles.note} role="status">
              {t("forget.loading")}
            </p>
          ) : null}
          {phase === "failedPreview" ? (
            <p className={styles.problem} role="alert">
              {t("forget.error.preview")}
            </p>
          ) : null}
          {refreshed && idle ? (
            <p className={styles.note} role="status">
              {t("forget.refreshed")}
            </p>
          ) : null}
          {failed && idle ? (
            <p className={styles.problem} role="alert">
              {t("forget.error.forget")}
            </p>
          ) : null}
          <Group
            title={t("journal.delete.group.kept")}
            rows={mine}
            ticked={ticked}
            cannot={cannot}
            disabled={phase === "working"}
            subheads
            onToggleGroup={(on) => toggleGroup(mine, on)}
            onToggleRow={toggleRow}
          />
          {others.length > 0 ? (
            <Group
              title={t("journal.delete.group.other")}
              note={t("journal.delete.group.otherNote")}
              rows={others}
              ticked={ticked}
              cannot={new Set()}
              disabled={phase === "working"}
              onToggleGroup={(on) => toggleGroup(others, on)}
              onToggleRow={toggleRow}
            />
          ) : null}
        </div>
        <div className={styles.foot}>
          {count > 0 ? (
            <ul className={styles.also}>
              {memories.length > 0 ? (
                <li>
                  <span>{t("journal.delete.memories", { count: memories.length })}</span>
                  <button
                    type="button"
                    className={styles.link}
                    aria-expanded={showMemories}
                    onClick={() => setShowMemories(!showMemories)}
                  >
                    {t(
                      showMemories
                        ? "journal.delete.memories.hide"
                        : "journal.delete.memories.show",
                    )}
                  </button>
                </li>
              ) : null}
              {memories.length > 0 && showMemories ? (
                <li className={styles.memoryList}>
                  <ul aria-label={t("journal.delete.memories.label")}>
                    {memories.map((m) => (
                      <li key={m.memoryId}>{m.sentence}</li>
                    ))}
                  </ul>
                </li>
              ) : null}
              {reflection ? (
                <li>
                  <span>{reflection}</span>
                </li>
              ) : null}
              <li>
                <span className={styles.small}>{t("forget.stays.out")}</span>
              </li>
            </ul>
          ) : null}
          {count === 0 && idle ? (
            <p className={styles.small}>{t("journal.delete.nothing")}</p>
          ) : null}
          <div
            className={styles.buttons}
            ref={(el) => {
              keep.current = el?.querySelector("button") ?? null;
            }}
          >
            <Button variant="secondary" disabled={phase === "working"} onClick={onClose}>
              {t("journal.delete.keep")}
            </Button>
            {phase === "failedPreview" ? (
              <Button variant="secondary" onClick={() => void prepare(ticked)}>
                {t("button.tryAgain")}
              </Button>
            ) : (
              <Button
                variant="forget"
                confirming
                disabled={!idle || !synced || count === 0}
                aria-busy={phase === "working"}
                onClick={() => void forget()}
              >
                {phase === "working"
                  ? t("forget.working")
                  : count === 0
                    ? t("journal.delete.button.none")
                    : t("journal.delete.button", { count })}
              </Button>
            )}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
