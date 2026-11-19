import { type FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { FeelingField, middleOf } from "../../components/FeelingField/FeelingField";
import { formatTime12 } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  CheckInInput,
  CheckIn as CheckInRecord,
  FeelingPart,
  FeelingPoint,
} from "../../engine/bindings";
import { FeelingMark } from "../../journal/FeelingMark";
import {
  type FeelingWord,
  MAIN_WORDS,
  nearestWords,
  PART_ORDER,
  PARTS,
  partOf,
  WORDS,
  wordsFor,
} from "../../journal/feelings";
import { feelingOf, MAX_WORDS, sentence, toggleWord } from "../../screens/checkin/words";
import {
  type Connection,
  ConnectionError,
  type PendingKeep,
  type PhoneEngine,
  PhoneRequestError,
  type Queue,
} from "../engine";
import { withKeepLock } from "../queue";
import { BottomSheet } from "./Dialog";
import { clientId } from "./Keep";
import { BackBar, Button, Icon, Page } from "./ui";

const WAIT_CODES: ReadonlySet<string> = new Set([
  "busy",
  "tooMany",
  "notReady",
  "paused",
  "sessionExpired",
]);

/** The window in which a check-in kept from the phone can be taken back, as a note's is. */
const TAKE_BACK_MS = 600_000;

type Mode = "field" | "list";

type State =
  | { name: "compose" }
  | { name: "sending" }
  | { name: "saved"; checkin: CheckInRecord; undone: boolean; late: boolean }
  | { name: "waiting"; at: number }
  | { name: "problem" };

interface CheckInProps {
  engine: PhoneEngine;
  queue: Queue;
  connection: Connection;
  deviceName: string;
  now(): number;
  /** "More words…" from Keep arrives with a part already chosen. */
  part?: FeelingPart;
  onBack(): void;
  onQueued(): void;
}

/** A light tap of vibration, only where the phone has one and the person has not asked for less motion. */
function tap() {
  const quiet = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
  if (quiet) return;
  globalThis.navigator?.vibrate?.(10);
}

function Meaning({ word }: { word: FeelingWord | null }) {
  return (
    <p className="ph-meaning" aria-live="polite">
      {word ? (
        <>
          <strong>{t(word.wordId)}</strong>
          {t("journal.checkin.meaning", { meaning: t(word.meaningId) })}
        </>
      ) : null}
    </p>
  );
}

/** Every word, grouped by part, 52 points a row, with the meaning under each word chosen. */
function WordList({ words, onToggle }: { words: readonly string[]; onToggle(word: string): void }) {
  const [open, setOpen] = useState<ReadonlySet<FeelingPart>>(new Set());
  const full = words.length >= MAX_WORDS;
  const flip = (part: FeelingPart) =>
    setOpen((now) => {
      const next = new Set(now);
      if (!next.delete(part)) next.add(part);
      return next;
    });
  return (
    <div className="ph-stack" style={{ gap: 18 }}>
      {PART_ORDER.map((part) => (
        <fieldset key={part} className="ph-lists" data-part={part}>
          <legend className="ph-lbl ph-lists-legend">
            <FeelingMark mark={part} size={16} />
            {t(PARTS[part].name)}
          </legend>
          {wordsFor(part, open.has(part)).map((word) => {
            const on = words.includes(word.word);
            return (
              <label key={word.id} className="ph-prow">
                <input
                  type="checkbox"
                  className="ph-prow-input"
                  checked={on}
                  aria-disabled={(full && !on) || undefined}
                  onChange={() => onToggle(word.word)}
                />
                <span className="ph-prow-text">
                  {t(word.wordId)}
                  {on ? <span className="ph-prow-note">{t(word.meaningId)}</span> : null}
                </span>
                <span className="ph-box" aria-hidden="true">
                  {on ? <Icon name="check" size={16} /> : null}
                </span>
              </label>
            );
          })}
          <button
            type="button"
            className="ph-link"
            aria-expanded={open.has(part)}
            onClick={() => flip(part)}
          >
            {t(open.has(part) ? "journal.checkin.fewer" : "journal.checkin.more")}
          </button>
        </fieldset>
      ))}
    </div>
  );
}

function Sheet({
  part,
  point,
  words,
  line,
  working,
  onToggle,
  onLine,
  onClose,
  onSave,
}: {
  part: FeelingPart;
  point: FeelingPoint;
  words: readonly string[];
  line: string;
  working: boolean;
  onToggle(word: string): void;
  onLine(next: string): void;
  onClose(): void;
  onSave(event: FormEvent): void;
}) {
  const [more, setMore] = useState(false);
  const [last, setLast] = useState<FeelingWord | null>(null);
  const name = t(PARTS[part].name);
  const nearest = nearestWords(part, point);
  const shown = more ? [...nearest, ...WORDS[part].slice(MAIN_WORDS)] : nearest.slice(0, 8);
  const full = words.length >= MAX_WORDS;
  return (
    <BottomSheet label={t("journal.checkin.words", { part: name })} onClose={onClose}>
      <form className="ph-sheet-body" onSubmit={onSave}>
        <div className="ph-row-between">
          <h2 className="ph-lbl ph-sheet-head">{t("journal.checkin.words", { part: name })}</h2>
          <span className="ph-hint">{t("phone.journal.checkin.upTo")}</span>
        </div>
        <fieldset className="ph-fieldset">
          <legend className="sr-only">{t("journal.checkin.words", { part: name })}</legend>
          <div className="ph-fwords">
            {shown.map((entry) => {
              const on = words.includes(entry.word);
              return (
                <button
                  key={entry.id}
                  type="button"
                  className="ph-fword"
                  data-part={part}
                  aria-pressed={on}
                  aria-disabled={(full && !on) || undefined}
                  onClick={() => {
                    setLast(entry);
                    onToggle(entry.word);
                  }}
                >
                  {t(entry.wordId)}
                </button>
              );
            })}
          </div>
        </fieldset>
        <Meaning word={last} />
        <button
          type="button"
          className="ph-link"
          aria-expanded={more}
          onClick={() => setMore(!more)}
        >
          {t(more ? "journal.checkin.fewer" : "journal.checkin.more")}
        </button>
        <input
          className="ph-input ph-input--note"
          value={line}
          aria-label={t("journal.checkin.going")}
          placeholder={t("journal.checkin.going")}
          autoComplete="off"
          onChange={(e) => onLine(e.target.value)}
        />
        <div className="ph-sheet-foot">
          <Button kind="primary" type="submit" disabled={working}>
            {t("journal.checkin.save")}
          </Button>
        </div>
      </form>
    </BottomSheet>
  );
}

/** The check-in for touch: the field filling the width, then the words in a sheet. */
export function CheckIn({
  engine,
  queue,
  connection,
  deviceName,
  now,
  part: startPart,
  onBack,
  onQueued,
}: CheckInProps) {
  const [state, setState] = useState<State>({ name: "compose" });
  const [mode, setMode] = useState<Mode>("field");
  const [point, setPoint] = useState<FeelingPoint | null>(startPart ? middleOf(startPart) : null);
  const [words, setWords] = useState<string[]>([]);
  const [line, setLine] = useState("");
  const [sheet, setSheet] = useState(startPart !== undefined);
  const dragging = useRef(false);
  const arrow = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);
  const connected = connection.kind === "connected";

  const key = state.name;
  useEffect(() => {
    void key;
    if (key !== "compose") heading.current?.focus();
  }, [key]);

  const place = (next: FeelingPoint) => {
    const part = partOf(next);
    if (point && partOf(point) !== part) tap();
    // Words belong to a part, so the ones left behind go when the point crosses into another.
    setWords((now) => now.filter((w) => WORDS[part].some((entry) => entry.word === w)));
    setPoint(next);
    // A press or a drag opens the words when the finger lifts; a part chosen by key opens them now.
    if (!dragging.current && !arrow.current) setSheet(true);
  };

  const feeling = feelingOf(mode, point, words);
  const closeSheet = useCallback(() => setSheet(false), []);

  const save = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!feeling) return;
    const input: CheckInInput = {
      feeling,
      line: line.trim() || null,
      urge: null,
      replaces: null,
    };
    const keep: PendingKeep = {
      clientId: clientId(),
      kind: "checkin",
      checkIn: input,
      size: input.feeling.words.length,
      createdAt: now(),
    };
    await queue.add(keep);
    onQueued();
    const wait = () => setState({ name: "waiting", at: keep.createdAt });
    if (!connected) return wait();
    setSheet(false);
    setState({ name: "sending" });
    const drop = async () => {
      await queue.remove(keep.clientId);
      onQueued();
    };
    try {
      const made = await withKeepLock(() => engine.keepCheckIn(input, keep.clientId));
      await drop();
      setState({ name: "saved", checkin: made, undone: false, late: false });
    } catch (e) {
      if (e instanceof ConnectionError) return wait();
      if (e instanceof PhoneRequestError) {
        if (WAIT_CODES.has(e.code)) return wait();
        await drop();
        return setState({ name: "problem" });
      }
      await drop();
      setState({ name: "problem" });
    }
  };

  const bar = <BackBar title={t("journal.side.checkIn")} onBack={onBack} />;

  if (state.name === "sending") {
    return (
      <Page bar={bar}>
        <main className="ph-main" aria-busy="true">
          <p className="ph-status" role="status" style={{ fontSize: "1rem" }}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
            {t("phone.keep.sending")}
          </p>
        </main>
      </Page>
    );
  }

  if (state.name === "problem") {
    return (
      <Page bar={bar}>
        <main className="ph-main">
          <div className="ph-live" role="alert" style={{ flex: "none" }}>
            <h1 className="ph-h1" tabIndex={-1} ref={heading}>
              {t("phone.keep.failed.title")}
            </h1>
            <p className="ph-body">{t("phone.keep.failed.body")}</p>
          </div>
          <div className="ph-spacer ph-stack">
            <Button kind="primary" onClick={() => setState({ name: "compose" })}>
              {t("phone.tryAgain")}
            </Button>
            <Button kind="quiet" onClick={onBack}>
              {t("phone.done")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  if (state.name === "waiting") {
    return (
      <Page bar={bar}>
        <main className="ph-main" style={{ paddingTop: 40, gap: 20 }}>
          <div className="ph-kept" role="status">
            <span
              className="ph-icon-box ph-icon-box--round"
              style={{ width: 44, height: 44 }}
              aria-hidden="true"
            >
              <Icon name="moon" size={20} />
            </span>
            <h1 className="ph-kept-word" tabIndex={-1} ref={heading}>
              {t("phone.keep.waiting")}
            </h1>
          </div>
          <p className="ph-body">{t("phone.keep.waiting.body")}</p>
          <p className="ph-rc" style={{ color: "var(--ink2)" }}>
            {t("phone.journal.checkin.waitingFrom", { time: formatTime12(new Date(state.at)) })}
          </p>
          <div className="ph-spacer ph-stack">
            <Button kind="primary" onClick={onBack}>
              {t("phone.done")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  if (state.name === "saved") {
    const { checkin } = state;
    const { part, words: kept } = checkin.feeling;
    const until = new Date(Date.parse(checkin.keptAt) + TAKE_BACK_MS);
    return (
      <Page bar={bar}>
        <main className="ph-main" style={{ paddingTop: 40, gap: 14 }}>
          <h1 className="ph-h1 ph-h1--small" tabIndex={-1} ref={heading}>
            {kept.length > 0
              ? t("journal.checkin.saved", { words: sentence(kept) })
              : t("journal.checkin.savedBare")}
          </h1>
          <p>
            <span className="ph-feelchip" data-part={part}>
              <FeelingMark mark={part} size={14} />
              {kept.length > 0
                ? t("journal.checkin.feel", { part: t(PARTS[part].name), words: kept.join(", ") })
                : t(PARTS[part].name)}
            </span>
          </p>
          <p className="ph-body">{t("journal.quick.keptMessage")}</p>
          <p className="ph-rc" style={{ color: "var(--ink2)" }}>
            {t("phone.journal.checkin.from", {
              time: formatTime12(checkin.keptAt),
              name: deviceName,
            })}
          </p>
          <div className="ph-card" style={{ gap: 10 }}>
            {state.undone ? (
              <span className="ph-rc" role="status">
                {t("phone.keep.undone")}
              </span>
            ) : state.late ? (
              <span className="ph-rc" role="status">
                {t("phone.keep.undo.late")}
              </span>
            ) : (
              <>
                <span className="ph-card-title">
                  {t("phone.keep.undo", { time: formatTime12(until) })}
                </span>
                <Button
                  kind="quiet"
                  onClick={async () => {
                    try {
                      await engine.unkeep(checkin.id);
                      setState({ ...state, undone: true });
                    } catch (e) {
                      if (e instanceof PhoneRequestError || e instanceof ConnectionError)
                        setState({ ...state, late: true });
                      else throw e;
                    }
                  }}
                >
                  {t("phone.keep.undo.button")}
                </Button>
              </>
            )}
          </div>
          <div className="ph-spacer ph-stack">
            <Button kind="primary" onClick={onBack}>
              {t("phone.done")}
            </Button>
          </div>
        </main>
      </Page>
    );
  }

  const part = point ? partOf(point) : null;
  const listFeeling = mode === "list" ? feeling : null;
  return (
    <Page bar={bar} className="ph-checkin">
      <main className="ph-main ph-main--tight" style={{ gap: 12, paddingBottom: 24 }}>
        {mode === "field" ? (
          <>
            {!sheet || !point ? (
              <>
                <h1 className="ph-h1 ph-h1--small">{t("journal.checkin.title")}</h1>
                <p className="ph-body" style={{ fontSize: "0.9375rem" }}>
                  {t("phone.journal.checkin.lead")}
                </p>
              </>
            ) : null}
            <div
              className={`ph-ci-field${sheet && point ? " ph-ci-field--small" : ""}`}
              onPointerDownCapture={() => {
                dragging.current = true;
              }}
              onPointerUpCapture={() => {
                if (!dragging.current) return;
                dragging.current = false;
                setSheet(true);
              }}
              onPointerCancelCapture={() => {
                dragging.current = false;
              }}
              onKeyDownCapture={(e) => {
                arrow.current = e.key.startsWith("Arrow");
              }}
              onKeyUpCapture={() => {
                arrow.current = false;
              }}
            >
              <FeelingField point={point} onChange={place} />
            </div>
            {!sheet || !point ? (
              <>
                {point && part ? (
                  <Button kind="quiet" onClick={() => setSheet(true)}>
                    {t("journal.checkin.words", { part: t(PARTS[part].name) })}
                  </Button>
                ) : null}
                <button type="button" className="ph-link" onClick={() => setMode("list")}>
                  {t("journal.checkin.showList")}
                </button>
              </>
            ) : null}
          </>
        ) : (
          <>
            <h1 className="ph-h1 ph-h1--small">{t("journal.checkin.title")}</h1>
            <button
              type="button"
              className="ph-link"
              onClick={() => {
                if (listFeeling) setPoint(listFeeling.point);
                setMode("field");
              }}
            >
              {t("journal.checkin.showField")}
            </button>
            <WordList words={words} onToggle={(w) => setWords(toggleWord(words, w))} />
            <p className="ph-hint">{t("journal.checkin.listHint")}</p>
            <input
              className="ph-input ph-input--note"
              value={line}
              aria-label={t("journal.checkin.going")}
              placeholder={t("journal.checkin.going")}
              autoComplete="off"
              onChange={(e) => setLine(e.target.value)}
            />
          </>
        )}
      </main>
      {mode === "list" ? (
        <footer className="ph-foot">
          <Button kind="primary" disabled={!feeling} onClick={() => void save()}>
            {t("journal.checkin.save")}
          </Button>
        </footer>
      ) : null}
      {mode === "field" && sheet && point && part ? (
        <Sheet
          part={part}
          point={point}
          words={words}
          line={line}
          working={false}
          onToggle={(w) => setWords(toggleWord(words, w))}
          onLine={setLine}
          onClose={closeSheet}
          onSave={(e) => void save(e)}
        />
      ) : null}
    </Page>
  );
}
