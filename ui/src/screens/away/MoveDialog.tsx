import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Children, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Progress } from "../../components/Progress/Progress";
import { formatBytes } from "../../copy/format";
import { t, tParts } from "../../copy/t";
import type { MoveChoice, MoveView } from "../../engine/client";
import { useEngine, useEngineEvent } from "../../engine/context";
import styles from "./Away.module.css";
import { useBusyAct } from "./useRemote";

export const MOVE_KEY = ["move"] as const;

const FOCUSABLE =
  "button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex='-1'])";
const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

/** "3 March", for a `YYYY-MM-DD` day. */
const dayAndMonth = (day: string) =>
  new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "long", timeZone: "UTC" }).format(
    new Date(`${day}T12:00:00Z`),
  );

/** How a phone joins a computer that already holds a memory. */
type Join = "bring" | "useHere";

/** A phone's memory moving here, as the computer last said it. */
export function useMove() {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: MOVE_KEY,
    queryFn: () => engine.getMove(),
    // While the question waits for room, the room is looked at again now and then.
    refetchInterval: (q) => {
      const view = q.state.data;
      return view?.phase === "offered" && view.room ? 3000 : false;
    },
  });
  useEngineEvent("move-changed", (view) => client.setQueryData(MOVE_KEY, view));
  return query.data ?? null;
}

/**
 * A phone that holds its own memory, offering it to this computer: the question (with a line
 * when there is not room yet), the memory arriving and being checked, which can be stopped, the
 * moment before Nocturne restarts to open it, and how it ended. A computer that already holds a
 * memory asks instead how the phone joins it: its things brought in (arriving, which can be
 * stopped, then added, which cannot) or this computer's memory used. Nothing changes until the
 * person chooses; Escape on a question means "Not now", and on "Stop this move?" means "Keep
 * moving".
 */
export function MoveDialog() {
  const view = useMove();
  if (!view) return null;
  return <MovePanel view={view} />;
}

function MovePanel({ view }: { view: MoveView }) {
  const engine = useEngine();
  const client = useQueryClient();
  const { run, busy } = useBusyAct();
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const answer = async (choice: MoveChoice) => {
    await run(() => engine.answerMove(choice));
    void client.invalidateQueries({ queryKey: MOVE_KEY });
  };
  const [stopping, setStopping] = useState(false);
  const [join, setJoin] = useState<Join>("bring");
  const offered = view.phase === "offered";
  const onEscape = useRef<(() => void) | null>(null);
  onEscape.current = busy
    ? null
    : stopping
      ? () => setStopping(false)
      : offered
        ? () => void answer("notNow")
        : null;
  const stoppable =
    view.phase === "preparing" || view.phase === "arriving" || view.phase === "checking";
  useEffect(() => {
    if (!stoppable) setStopping(false);
  }, [stoppable]);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const root = document.getElementById("root");
    root?.setAttribute("inert", "");
    panel.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        onEscape.current?.();
        return;
      }
      if (event.key !== "Tab" || !panel.current) return;
      const focusable = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) {
        event.preventDefault();
        return;
      }
      const active = document.activeElement;
      if (event.shiftKey && (active === first || active === panel.current)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      root?.removeAttribute("inert");
      opener?.focus?.();
    };
  }, []);

  const name = view.name;
  const bring = "bring" in view && view.bring === true;
  const finish = (then?: () => void) =>
    void run(() => engine.finishMove()).then((done) => {
      void client.invalidateQueries({ queryKey: MOVE_KEY });
      if (done) then?.();
    });
  const stopButton = (
    <Button variant="secondary" disabled={busy} onClick={() => setStopping(true)}>
      {t("moving.receive.stop")}
    </Button>
  );
  let title = t("moving.receive.title", { name });
  let body: ReactNode = null;
  let foot: ReactNode = null;
  switch (view.phase) {
    case "offered": {
      if (view.here) {
        const room = join === "bring" ? (view.room ?? null) : null;
        title = t("moving.choose.title", { name });
        const options: { value: Join; label: string; hint: string }[] = [
          {
            value: "bring",
            label: t("moving.choose.bring"),
            hint: t("moving.choose.bring.hint", { name }),
          },
          {
            value: "useHere",
            label: t("moving.choose.use"),
            hint: t("moving.choose.use.hint", { name }),
          },
        ];
        body = (
          <>
            <p className={styles.muted}>
              {view.since
                ? t("moving.choose.counts", {
                    count: count(view.items),
                    name,
                    date: dayAndMonth(view.since),
                    size: formatBytes(view.bytes),
                  })
                : t("moving.receive.counts", {
                    count: count(view.items),
                    size: formatBytes(view.bytes),
                  })}
            </p>
            <p>{t("moving.choose.lead", { name })}</p>
            <div
              className={styles.joins}
              role="radiogroup"
              aria-label={t("moving.choose.title", { name })}
            >
              {options.map((o) => (
                <label key={o.value} className={styles.join}>
                  <input
                    type="radio"
                    name="join"
                    value={o.value}
                    checked={join === o.value}
                    onChange={() => setJoin(o.value)}
                  />
                  <span className={styles.joinText}>
                    <span className={styles.joinLabel}>{o.label}</span>
                    <span>{o.hint}</span>
                  </span>
                </label>
              ))}
            </div>
            {room ? (
              <p role="status">
                {Children.toArray(
                  tParts("moving.receive.room", {
                    size: formatBytes(room.need),
                    free: formatBytes(room.free),
                    moveItHere: <strong>{t("moving.choose.continue")}</strong>,
                  }) as ReactNode[],
                )}
              </p>
            ) : null}
          </>
        );
        foot = (
          <>
            <Button variant="secondary" disabled={busy} onClick={() => void answer("notNow")}>
              {t("moving.receive.notNow")}
            </Button>
            <Button
              variant="primary"
              disabled={busy || room !== null}
              onClick={() => void answer(join)}
            >
              {t("moving.choose.continue")}
            </Button>
          </>
        );
        break;
      }
      const room = view.room ?? null;
      body = (
        <>
          <p className={styles.muted}>
            {t("moving.receive.counts", {
              count: count(view.items),
              size: formatBytes(view.bytes),
            })}
          </p>
          <p>{t("moving.receive.lead", { name })}</p>
          {room ? (
            <p role="status">
              {Children.toArray(
                tParts("moving.receive.room", {
                  size: formatBytes(room.need),
                  free: formatBytes(room.free),
                  moveItHere: <strong>{t("moving.receive.move")}</strong>,
                }) as ReactNode[],
              )}
            </p>
          ) : null}
        </>
      );
      foot = (
        <>
          <Button variant="secondary" disabled={busy} onClick={() => void answer("notNow")}>
            {t("moving.receive.notNow")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || room !== null}
            onClick={() => void answer("move")}
          >
            {t("moving.receive.move")}
          </Button>
        </>
      );
      break;
    }
    case "preparing":
      if (bring) title = t("moving.choose.title", { name });
      body = (
        <Progress
          value={null}
          max={1}
          label={t(bring ? "moving.bring.preparing" : "moving.receive.preparing", { name })}
        />
      );
      foot = stopButton;
      break;
    case "arriving":
      if (bring) title = t("moving.choose.title", { name });
      body = (
        <Progress
          value={view.done}
          max={view.size}
          label={t(bring ? "moving.bring.arriving" : "moving.receive.arriving", {
            name,
            done: formatBytes(view.done),
            size: formatBytes(view.size),
          })}
        />
      );
      foot = stopButton;
      break;
    // Stop is no longer offered: what arrived is being added in one step.
    case "adding":
      title = t("moving.choose.title", { name });
      body = <Progress value={null} max={1} label={t("moving.bring.adding")} />;
      break;
    case "brought":
      title = t("moving.brought.title", { name });
      body = (
        <p>
          {t("moving.brought.body", {
            count: count(view.added.things),
            entries: count(view.added.entries),
            checkins: count(view.added.checkins),
          })}
        </p>
      );
      foot = (
        <Button variant="primary" disabled={busy} onClick={() => finish(() => goTo("/today"))}>
          {t("moving.arrived.open")}
        </Button>
      );
      break;
    case "used":
      title = t("moving.used.title", { name });
      body = <p>{t("moving.used.body")}</p>;
      foot = (
        <Button variant="primary" disabled={busy} onClick={() => finish()}>
          {t("moving.failed.ok")}
        </Button>
      );
      break;
    case "checking":
      body = <Progress value={1} max={1} label={t("moving.receive.checking", { name })} />;
      foot = stopButton;
      break;
    // The line is the whole of it: the question above it is over.
    case "restarting":
      title = t("moving.receive.restarting", { name });
      break;
    case "failed":
      title =
        view.why === "noRoom"
          ? t("moving.failed.room", { name, size: formatBytes(view.room?.need ?? 0) })
          : t("moving.failed.notOpened", { name });
      foot = (
        <Button
          variant="primary"
          disabled={busy}
          onClick={() =>
            void run(() => engine.finishMove()).then(() =>
              client.invalidateQueries({ queryKey: MOVE_KEY }),
            )
          }
        >
          {t("moving.failed.ok")}
        </Button>
      );
      break;
    case "arrived":
      title = t("moving.arrived.title", { name });
      body = (
        <>
          <p>{t("moving.arrived.body")}</p>
          {view.online ? null : <p className={styles.muted}>{tc("moving.arrived.online")}</p>}
        </>
      );
      foot = (
        <Button
          variant="primary"
          disabled={busy}
          onClick={() =>
            void run(() => engine.finishMove()).then((done) => {
              if (done) goTo("/today");
            })
          }
        >
          {t("moving.arrived.open")}
        </Button>
      );
      break;
  }
  if (stopping && stoppable) {
    body = <p>{t(bring ? "moving.bring.stopAsk" : "moving.receive.stopAsk", { name })}</p>;
    foot = (
      <>
        <Button variant="secondary" disabled={busy} onClick={() => setStopping(false)}>
          {t(bring ? "moving.bring.keepGoing" : "moving.receive.keepMoving")}
        </Button>
        <Button
          variant="forget"
          confirming
          disabled={busy}
          onClick={() =>
            void run(() => engine.stopMove()).then(() => {
              setStopping(false);
              void client.invalidateQueries({ queryKey: MOVE_KEY });
            })
          }
        >
          {t(bring ? "moving.bring.stop" : "moving.receive.stopMoving")}
        </Button>
      </>
    );
  }

  return createPortal(
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.pairing}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-busy={stoppable || view.phase === "restarting" || view.phase === "adding"}
        tabIndex={-1}
      >
        <h2 id={titleId} className={styles.statement}>
          {title}
        </h2>
        <div className={styles.pairBody}>{body}</div>
        {foot ? <div className={styles.moveFoot}>{foot}</div> : null}
      </div>
    </div>,
    document.body,
  );
}
