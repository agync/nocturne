import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BroughtCounts, MemoryCounts } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { BottomSheet } from "../../screens/Dialog";
import { Button, Icon, Page } from "../../screens/ui";

/** What would move: the store's counts, the first day something was kept, about how big. */
export interface MoveFacts {
  counts: MemoryCounts;
  /** Things kept, apart from the journal, check-ins, the calendar and readings. */
  things: number;
  /** Journal entries and good things. */
  journal: number;
  since: string | null;
  bytes: number;
  /** Of that, what the photos and recordings take. */
  media: number;
}

/**
 * How the move goes: getting it ready, sending, the computer checking, paused, or finishing while
 * Nocturne on the computer restarts to open it.
 */
export interface MoveProgress {
  phase: "preparing" | "sending" | "checking" | "paused" | "finishing";
  done: number;
  total: number;
}

/** How long the move is measured before it says how long is left. */
export const MEASURE_MS = 10_000;

/** "3 March", for a `YYYY-MM-DD` day. */
const dayAndMonth = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "long", timeZone: "UTC" }).format(
    new Date(`${day}T12:00:00Z`),
  );

/** Whole minutes left for `left` bytes at `perSecond`, never less than one. */
export function minutesLeft(left: number, perSecond: number): number {
  return Math.max(1, Math.ceil(left / perSecond / 60));
}

function Hero({ title, icon }: { title: string; icon?: "laptop" | "check" }) {
  const heading = useHeading(title);
  return (
    <>
      {icon ? (
        <span className="ph-hero-icon">
          <Icon name={icon} size={36} />
        </span>
      ) : null}
      <h1 className="ph-steph" tabIndex={-1} ref={heading}>
        {title}
      </h1>
    </>
  );
}

/** "Move your memory to {computer}?" */
export function MoveFound({
  computer,
  onContinue,
  onNotNow,
}: {
  computer: string;
  onContinue(): void;
  onNotNow(): void;
}) {
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 32 }}>
        <Hero title={t("moving.phone.found.title", { computer })} icon="laptop" />
        <p className="ph-body">{t("moving.phone.found.lead", { computer })}</p>
        <p className="ph-body">{t("moving.phone.found.then", { computer })}</p>
        <p className="ph-small">{t("moving.phone.found.one")}</p>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={onContinue}>
          {t("moving.phone.found.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNotNow}>
          {t("moving.phone.notNow")}
        </button>
      </footer>
    </Page>
  );
}

/**
 * "{computer} already holds a memory.": the person there chooses how this phone joins it, and
 * this one waits. Cancel undoes the pairing and leaves the phone as it was.
 */
export function MoveWaiting({ computer, onCancel }: { computer: string; onCancel(): void }) {
  const waiting = t("moving.phone.waiting.wait", { computer });
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy="true" style={{ gap: 14, paddingTop: 32 }}>
        <Hero title={t("moving.phone.waiting.title", { computer })} icon="laptop" />
        <p className="ph-body">{t("moving.phone.waiting.body", { computer })}</p>
        <div
          className="ph-progress ph-progress--unknown"
          role="progressbar"
          aria-label={waiting}
          aria-valuetext={waiting}
        >
          <span />
        </div>
        <p className="ph-small" role="status">
          {waiting}
        </p>
      </main>
      <footer className="ph-foot">
        <Button onClick={onCancel}>{t("phone.cancel")}</Button>
      </footer>
    </Page>
  );
}

/** "What moves": how much, how long, and what changes here once it has arrived. */
export function MoveSize({
  facts,
  onMove,
  onBack,
}: {
  facts: MoveFacts;
  onMove(): void;
  onBack(): void;
}) {
  const { counts, things, journal, since, bytes, media } = facts;
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 32 }}>
        <Hero title={t("moving.phone.size.title")} />
        <ul className="ph-see">
          {since ? (
            <li>
              {t("moving.phone.size.kept", {
                count: things,
                date: dayAndMonth(since),
              })}
            </li>
          ) : null}
          {journal > 0 ? <li>{t("moving.phone.size.journal", { count: journal })}</li> : null}
          {counts.pages > 0 ? (
            <li>{t("moving.phone.size.pages", { count: counts.pages })}</li>
          ) : null}
          <li>
            {t(media * 2 >= bytes ? "moving.phone.size.bytes" : "moving.phone.size.bytesOnly", {
              size: formatBytes(bytes),
            })}
          </li>
        </ul>
        <p className="ph-small">{t("moving.phone.size.open")}</p>
        <p className="ph-body">{t("moving.phone.size.after")}</p>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={onMove}>
          {t("moving.phone.size.move")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("moving.phone.back")}
        </button>
      </footer>
    </Page>
  );
}

/**
 * How long is left, once the move has been measured for `MEASURE_MS`: none before, since the
 * speed isn't known yet.
 */
function useLeft(progress: MoveProgress | null, now: () => number): number | null {
  const start = useRef<{ at: number; done: number } | null>(null);
  // Measured afresh after a pause, or when the move starts again from the beginning.
  if (progress?.phase !== "sending" || (start.current && progress.done < start.current.done)) {
    start.current = null;
  }
  if (progress?.phase !== "sending") return null;
  if (!start.current) start.current = { at: now(), done: progress.done };
  const elapsed = now() - start.current.at;
  const gone = progress.done - start.current.done;
  if (elapsed < MEASURE_MS || gone <= 0) return null;
  return minutesLeft(progress.total - progress.done, (gone * 1000) / elapsed);
}

/**
 * "Moving your memory": how far and, once measured, how long is left; kept things waiting; Stop
 * with its question. While the computer restarts to open it, it says so, and there is nothing
 * left to stop. With `bring`, "Bringing your things to {computer}": once everything went, the
 * computer adds them to its memory, and there is nothing left to stop either.
 */
export function MoveMoving({
  computer,
  bring = false,
  progress,
  onStop,
  now = Date.now,
}: {
  computer: string;
  bring?: boolean;
  progress: MoveProgress | null;
  onStop(): void;
  now?: () => number;
}) {
  const [asking, setAsking] = useState(false);
  const title = bring
    ? t("moving.phone.bringing.title", { computer })
    : t("moving.phone.moving.title");
  const ready = bring ? t("moving.phone.bringing.preparing") : t("moving.phone.moving.preparing");
  const sent = progress && progress.phase !== "preparing" ? progress : null;
  // Until the first part goes: the copy that travels is being made, and its size is not known.
  const preparing = !progress || progress.phase === "preparing";
  const share = sent && sent.total > 0 ? Math.min(1, sent.done / sent.total) : 0;
  const paused = progress?.phase === "paused";
  const finishing = progress?.phase === "finishing";
  const adding = bring && progress?.phase === "checking";
  const settled = finishing || adding;
  const left = useLeft(progress, now);
  useEffect(() => {
    if (settled) setAsking(false);
  }, [settled]);
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy="true" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 72 }}>
          <span className="ph-mark" aria-hidden="true" />
          <Hero title={title} />
        </div>
        {preparing ? (
          <div
            className="ph-progress ph-progress--unknown"
            role="progressbar"
            aria-label={title}
            aria-valuetext={ready}
          >
            <span />
          </div>
        ) : (
          <div
            className="ph-progress"
            role="progressbar"
            aria-label={title}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(share * 100)}
          >
            <span style={{ width: `${Math.round(share * 100)}%` }} />
          </div>
        )}
        {preparing ? <p className="ph-small">{ready}</p> : null}
        {sent && !paused && !settled ? (
          <p className="ph-small">
            {t("moving.phone.moving.progress", {
              done: formatBytes(sent.done),
              size: formatBytes(sent.total),
            })}
            {left !== null ? (
              <>
                <br />
                {t("moving.phone.moving.left", { minutes: left })}
              </>
            ) : null}
          </p>
        ) : null}
        {paused ? (
          <p className="ph-small" role="status">
            {t("moving.phone.paused", { computer })}
          </p>
        ) : null}
        {finishing ? (
          <p className="ph-body" role="status">
            {t("moving.phone.moving.finishing", { computer })}
          </p>
        ) : adding ? (
          <p className="ph-body" role="status">
            {t("moving.phone.bringing.adding", { computer })}
          </p>
        ) : bring ? null : (
          <p className="ph-small">{t("moving.phone.moving.note")}</p>
        )}
      </main>
      {settled ? null : (
        <footer className="ph-foot">
          <Button onClick={() => setAsking(true)}>{t("moving.phone.moving.stop")}</Button>
        </footer>
      )}
      {asking ? (
        <BottomSheet label={t("moving.phone.moving.stop")} onClose={() => setAsking(false)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 16 }}>
            <p className="ph-body">
              {t(bring ? "moving.phone.bringing.stopAsk" : "moving.phone.moving.stopAsk", {
                computer,
              })}
            </p>
            <Button kind="danger" onClick={onStop}>
              {t(bring ? "moving.phone.bringing.stop" : "moving.phone.moving.stopMoving")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
              {t(bring ? "moving.phone.bringing.keepGoing" : "moving.phone.moving.keepMoving")}
            </button>
          </div>
        </BottomSheet>
      ) : null}
    </Page>
  );
}

/** Why a move ended with nothing changed, as the phone says it. */
export type MoveEnded =
  | "notWhole"
  | "outdated"
  | "noRoom"
  | "ranOut"
  | "notOpened"
  | "stoppedThere"
  /** The person here stopped bringing the things in. */
  | "stoppedHere"
  /** The computer removed this phone part way. */
  | "removed";

/**
 * A move that ended with nothing changed: why, and "Try again" with "Not now", or "OK" when it
 * was stopped on either side or the computer removed this phone. `bring`: it was bringing this
 * phone's things in.
 */
export function MoveFailed({
  computer,
  why,
  bring = false,
  onTryAgain,
  onBack,
}: {
  computer: string;
  why: MoveEnded;
  bring?: boolean;
  onTryAgain(): void;
  onBack(): void;
}) {
  const line = {
    notWhole: t("moving.phone.notWhole"),
    outdated: t("moving.phone.outdated", { computer }),
    noRoom: t("moving.phone.noRoom", { computer }),
    ranOut: t("moving.phone.ranOut", { computer }),
    notOpened: t("moving.phone.notOpened", { computer }),
    stoppedThere: t("moving.phone.stoppedThere", { computer }),
    stoppedHere: t("moving.phone.bringing.stopped", { computer }),
    removed: t("phone.removed.body"),
  }[why];
  const final = why === "stoppedThere" || why === "stoppedHere" || why === "removed";
  const title =
    why === "removed"
      ? t("phone.removed.title")
      : bring
        ? t("moving.phone.bringing.title", { computer })
        : t("moving.phone.moving.title");
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 72 }}>
        <Hero title={title} />
        <p className="ph-body" role="alert">
          {line}
        </p>
      </main>
      <footer className="ph-foot">
        {final ? (
          <Button kind="primary" onClick={onBack}>
            {t("moving.phone.ok")}
          </Button>
        ) : (
          <>
            {why === "outdated" ? null : (
              <Button kind="primary" onClick={onTryAgain}>
                {t("moving.phone.tryAgain")}
              </Button>
            )}
            <button type="button" className="ph-textbtn" onClick={onBack}>
              {t("moving.phone.notNow")}
            </button>
          </>
        )}
      </footer>
    </Page>
  );
}

/**
 * "Your memory is on {computer}.", or with `brought` "Your things are on {computer}.", and what
 * becomes of the memory put aside here.
 */
export function MoveMoved({
  computer,
  items,
  brought = null,
  busy,
  onDone,
}: {
  computer: string;
  items: number;
  brought?: BroughtCounts | null;
  busy: boolean;
  onDone(remove: boolean): void;
}) {
  const [remove, setRemove] = useState(true);
  const choice = t("moving.phone.moved.choice");
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 40 }} role="status">
          {brought ? (
            <>
              <Hero title={t("moving.phone.brought.title", { computer })} icon="check" />
              <p className="ph-body">
                {t("moving.phone.brought.body", {
                  count: brought.things,
                  entries: brought.entries,
                  checkins: brought.checkins,
                  computer,
                })}
              </p>
            </>
          ) : (
            <>
              <Hero title={t("moving.phone.moved.title", { computer })} icon="check" />
              <p className="ph-body">{t("moving.phone.moved.body", { count: items, computer })}</p>
            </>
          )}
        </div>
        <fieldset className="ph-options" aria-label={choice}>
          <legend className="ph-lbl">{choice}</legend>
          <label className="ph-option">
            <input
              type="radio"
              name="put-aside"
              checked={remove}
              onChange={() => setRemove(true)}
            />
            <span className="ph-radio" aria-hidden="true" />
            <span>{t("moving.phone.moved.remove")}</span>
          </label>
          <label className="ph-option">
            <input
              type="radio"
              name="put-aside"
              checked={!remove}
              onChange={() => setRemove(false)}
            />
            <span className="ph-radio" aria-hidden="true" />
            <span>{t("moving.phone.moved.keep")}</span>
          </label>
        </fieldset>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={busy} onClick={() => onDone(remove)}>
          {t("moving.phone.moved.done")}
        </Button>
      </footer>
    </Page>
  );
}

/**
 * "This phone now works with {computer}.": nothing went, and what this phone held is put aside
 * here, kept unless the person removes it, which deletes it for good.
 */
export function MoveUsed({
  computer,
  busy,
  onDone,
}: {
  computer: string;
  busy: boolean;
  onDone(remove: boolean): void;
}) {
  const [remove, setRemove] = useState(false);
  const choice = t("moving.phone.moved.choice");
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 40 }} role="status">
          <Hero title={t("moving.phone.used.title", { computer })} icon="check" />
          <p className="ph-body">{t("moving.phone.used.body", { computer })}</p>
        </div>
        <fieldset className="ph-options" aria-label={choice}>
          <legend className="ph-lbl">{choice}</legend>
          <label className="ph-option">
            <input
              type="radio"
              name="put-aside"
              checked={!remove}
              onChange={() => setRemove(false)}
            />
            <span className="ph-radio" aria-hidden="true" />
            <span>{t("moving.phone.used.keep")}</span>
          </label>
          <label className="ph-option">
            <input
              type="radio"
              name="put-aside"
              checked={remove}
              onChange={() => setRemove(true)}
            />
            <span className="ph-radio" aria-hidden="true" />
            <span>{t("moving.phone.used.remove")}</span>
          </label>
        </fieldset>
        <p className="ph-small">{t("moving.phone.used.warning", { computer })}</p>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={busy} onClick={() => onDone(remove)}>
          {t("moving.phone.moved.done")}
        </Button>
      </footer>
    </Page>
  );
}
