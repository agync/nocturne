import { useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { MemoryCounts } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { BottomSheet } from "../../screens/Dialog";
import { Button, Icon, Page } from "../../screens/ui";

/** What would move: the store's counts, the first day something was kept, about how big. */
export interface MoveFacts {
  counts: MemoryCounts;
  since: string | null;
  bytes: number;
}

/** How the move goes: getting it ready, sending, the computer checking, or paused. */
export interface MoveProgress {
  phase: "preparing" | "sending" | "checking" | "paused";
  done: number;
  total: number;
}

/** The pace "About {minutes} minutes" is worked out at, in bytes a second. */
const ASSUMED_PACE = 2_000_000;

const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

/** "3 March", for a `YYYY-MM-DD` day. */
const dayAndMonth = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "long", timeZone: "UTC" }).format(
    new Date(`${day}T12:00:00Z`),
  );

/** Whole minutes a move of `bytes` takes at the assumed pace, never less than one. */
export function moveMinutes(bytes: number): number {
  return Math.max(1, Math.ceil(bytes / ASSUMED_PACE / 60));
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
  const { counts, since, bytes } = facts;
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 32 }}>
        <Hero title={t("moving.phone.size.title")} />
        <ul className="ph-see">
          {since ? (
            <li>
              {t("moving.phone.size.kept", {
                count: count(counts.items),
                date: dayAndMonth(since),
              })}
            </li>
          ) : null}
          {counts.pages > 0 ? (
            <li>{t("moving.phone.size.pages", { count: count(counts.pages) })}</li>
          ) : null}
          <li>{t("moving.phone.size.bytes", { size: formatBytes(bytes) })}</li>
        </ul>
        <p className="ph-small">
          {t("moving.phone.size.time", { minutes: String(moveMinutes(bytes)) })}
        </p>
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

/** "Moving your memory": how far, kept things waiting, and Stop with its question. */
export function MoveMoving({
  computer,
  progress,
  onStop,
}: {
  computer: string;
  progress: MoveProgress | null;
  onStop(): void;
}) {
  const [asking, setAsking] = useState(false);
  const title = t("moving.phone.moving.title");
  const sent = progress && progress.phase !== "preparing" ? progress : null;
  const share = sent && sent.total > 0 ? Math.min(1, sent.done / sent.total) : 0;
  const paused = progress?.phase === "paused";
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
        {sent && !paused ? (
          <p className="ph-small">
            {t("moving.phone.moving.progress", {
              done: formatBytes(sent.done),
              size: formatBytes(sent.total),
            })}
          </p>
        ) : null}
        {paused ? (
          <p className="ph-small" role="status">
            {t("moving.phone.paused", { computer })}
          </p>
        ) : null}
        <p className="ph-small">{t("moving.phone.moving.note")}</p>
      </main>
      <footer className="ph-foot">
        <Button onClick={() => setAsking(true)}>{t("moving.phone.moving.stop")}</Button>
      </footer>
      {asking ? (
        <BottomSheet label={t("moving.phone.moving.stop")} onClose={() => setAsking(false)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 16 }}>
            <p className="ph-body">{t("moving.phone.moving.stopAsk", { computer })}</p>
            <Button kind="primary" onClick={onStop}>
              {t("moving.phone.moving.stop")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
              {t("moving.phone.back")}
            </button>
          </div>
        </BottomSheet>
      ) : null}
    </Page>
  );
}

/** A move that did not finish: nothing changed, and it can be tried again. */
export function MoveFailed({
  computer,
  outdated,
  onTryAgain,
  onBack,
}: {
  computer: string;
  outdated: boolean;
  onTryAgain(): void;
  onBack(): void;
}) {
  const line = outdated ? t("moving.phone.outdated", { computer }) : t("moving.phone.notWhole");
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14, paddingTop: 72 }}>
        <Hero title={t("moving.phone.moving.title")} />
        <p className="ph-body" role="alert">
          {line}
        </p>
      </main>
      <footer className="ph-foot">
        {outdated ? null : (
          <Button kind="primary" onClick={onTryAgain}>
            {t("moving.phone.tryAgain")}
          </Button>
        )}
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("moving.phone.back")}
        </button>
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
