import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { MemoryCounts } from "../../../engine/bindings";
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

const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

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
  const { counts, things, journal, since, bytes } = facts;
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 32 }}>
        <Hero title={t("moving.phone.size.title")} />
        <ul className="ph-see">
          {since ? (
            <li>
              {t("moving.phone.size.kept", {
                count: count(things),
                date: dayAndMonth(since),
              })}
            </li>
          ) : null}
          {journal > 0 ? (
            <li>{t("moving.phone.size.journal", { count: count(journal) })}</li>
          ) : null}
          {counts.pages > 0 ? (
            <li>{t("moving.phone.size.pages", { count: count(counts.pages) })}</li>
          ) : null}
          <li>{t("moving.phone.size.bytes", { size: formatBytes(bytes) })}</li>
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
 * left to stop.
 */
export function MoveMoving({
  computer,
  progress,
  onStop,
  now = Date.now,
}: {
  computer: string;
  progress: MoveProgress | null;
  onStop(): void;
  now?: () => number;
}) {
  const [asking, setAsking] = useState(false);
  const title = t("moving.phone.moving.title");
  const sent = progress && progress.phase !== "preparing" ? progress : null;
  const share = sent && sent.total > 0 ? Math.min(1, sent.done / sent.total) : 0;
  const paused = progress?.phase === "paused";
  const finishing = progress?.phase === "finishing";
  const left = useLeft(progress, now);
  useEffect(() => {
    if (finishing) setAsking(false);
  }, [finishing]);
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy="true" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 72 }}>
          <span className="ph-mark" aria-hidden="true" />
          <Hero title={title} />
        </div>
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
        {sent && !paused && !finishing ? (
          <p className="ph-small">
            {t("moving.phone.moving.progress", {
              done: formatBytes(sent.done),
              size: formatBytes(sent.total),
            })}
            {left !== null ? (
              <>
                <br />
                {t("moving.phone.moving.left", { minutes: String(left) })}
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
        ) : (
          <p className="ph-small">{t("moving.phone.moving.note")}</p>
        )}
      </main>
      {finishing ? null : (
        <footer className="ph-foot">
          <Button onClick={() => setAsking(true)}>{t("moving.phone.moving.stop")}</Button>
        </footer>
      )}
      {asking ? (
        <BottomSheet label={t("moving.phone.moving.stop")} onClose={() => setAsking(false)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 16 }}>
            <p className="ph-body">{t("moving.phone.moving.stopAsk", { computer })}</p>
            <Button kind="danger" onClick={onStop}>
              {t("moving.phone.moving.stopMoving")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
              {t("moving.phone.moving.keepMoving")}
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
  | "stoppedThere";

/**
 * A move that ended with nothing changed: why, and "Try again" with "Not now", or "OK" when the
 * person on the computer stopped it.
 */
export function MoveFailed({
  computer,
  why,
  onTryAgain,
  onBack,
}: {
  computer: string;
  why: MoveEnded;
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
  }[why];
  const final = why === "stoppedThere";
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 72 }}>
        <Hero title={t("moving.phone.moving.title")} />
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

/** "Your memory is on {computer}.", and what becomes of the memory put aside here. */
export function MoveMoved({
  computer,
  items,
  busy,
  onDone,
}: {
  computer: string;
  items: number;
  busy: boolean;
  onDone(remove: boolean): void;
}) {
  const [remove, setRemove] = useState(true);
  const choice = t("moving.phone.moved.choice");
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 40 }} role="status">
          <Hero title={t("moving.phone.moved.title", { computer })} icon="check" />
          <p className="ph-body">
            {t("moving.phone.moved.body", { count: count(items), computer })}
          </p>
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
