import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Progress } from "../../components/Progress/Progress";
import { formatBytes } from "../../copy/format";
import { t } from "../../copy/t";
import type { MoveChoice, MoveView } from "../../engine/client";
import { useEngine, useEngineEvent } from "../../engine/context";
import styles from "./Away.module.css";
import { useBusyAct } from "./useRemote";

export const MOVE_KEY = ["move"] as const;

const FOCUSABLE = "button:not([disabled]), [href], [tabindex]:not([tabindex='-1'])";
const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

/** A phone's memory moving here, as the computer last said it. */
export function useMove() {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({ queryKey: MOVE_KEY, queryFn: () => engine.getMove() });
  useEngineEvent("move-changed", (view) => client.setQueryData(MOVE_KEY, view));
  return query.data ?? null;
}

/**
 * A phone that holds its own memory, offering it to this computer: the question, the memory
 * arriving and being checked, and that it is here. Nothing changes until the person chooses;
 * Escape on the question means "Not now" or "Keep what's here". Once the memory is on its way the
 * dialog stays until it is here.
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
  const offered = view.phase === "offered";
  const onEscape = useRef<(() => void) | null>(null);
  onEscape.current = offered && !busy ? () => void answer("notNow") : null;

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
  let title: string;
  let body: React.ReactNode;
  let foot: React.ReactNode;
  switch (view.phase) {
    case "offered":
      title = t("moving.receive.title", { name });
      body = (
        <>
          <p className={styles.muted}>
            {t("moving.receive.counts", {
              count: count(view.items),
              size: formatBytes(view.bytes),
            })}
          </p>
          <p>{t("moving.receive.lead", { name })}</p>
          {view.here ? <p>{t("moving.receive.already", { name })}</p> : null}
          {view.here && view.others ? <p>{t("moving.receive.others")}</p> : null}
        </>
      );
      foot = (
        <>
          <Button variant="secondary" disabled={busy} onClick={() => void answer("notNow")}>
            {t(view.here ? "moving.receive.keep" : "moving.receive.notNow")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void answer("move")}>
            {t(view.here ? "moving.receive.putAside" : "moving.receive.move")}
          </Button>
        </>
      );
      break;
    case "arriving":
      title = t("moving.receive.title", { name });
      body = (
        <Progress
          value={view.done}
          max={view.size}
          label={t("moving.receive.arriving", {
            name,
            done: formatBytes(view.done),
            size: formatBytes(view.size),
          })}
        />
      );
      foot = null;
      break;
    case "checking":
      title = t("moving.receive.title", { name });
      body = <Progress value={1} max={1} label={t("moving.receive.checking", { name })} />;
      foot = null;
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

  return createPortal(
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.pairing}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-busy={view.phase === "arriving" || view.phase === "checking"}
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
