import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { BroughtCounts, MemoryCounts } from "../../../engine/bindings";
import {
  type MoveEnded,
  type MoveFacts,
  MoveFailed,
  MoveFound,
  MoveMoved,
  MoveMoving,
  type MoveProgress,
  MoveSize,
  MoveUsed,
  MoveWaiting,
} from "./MoveScreens";

/** Why a move did not finish. Nothing changed on the phone in any of them. */
export type MoveError =
  | "paused"
  | "notWhole"
  | "computerOutdated"
  | "declined"
  | "stopped"
  | "full"
  | "failed"
  | "noRoom"
  | "ranOutOfRoom"
  | "notOpened"
  | "stoppedThere";

/** The computer's answer to the offer. */
export interface MoveOffered {
  /** It already opened there, or its things were added: the move only has to finish. */
  opened: boolean;
  here: MemoryCounts | null;
  /** The person there chose to bring this phone's things in: the move starts by itself. */
  bring?: boolean;
  /** The person there chose the computer's memory: what was here is already put aside. */
  useHere?: boolean;
}

/** A finished move: how many things arrived, and with `brought` what was added there. */
export interface Moved {
  items: number;
  brought?: BroughtCounts | null;
}

/** The phone's way to move its memory to the computer it has just paired with. */
export interface MoveApi {
  /** The computer's name, from the session the pairing opened. */
  computer(): Promise<string>;
  /** The computer already holds a memory: the person there chooses how this phone joins it. */
  holds(): Promise<boolean>;
  facts(): Promise<MoveFacts>;
  /**
   * Offers the memory and waits for the person on the computer: what it holds, or that the
   * memory already opened there (a move whose phone went away part way, which only has to
   * finish).
   */
  offer(): Promise<MoveOffered>;
  /** "Move my memory": answers how many things arrived. Rejects with a `MoveError`. */
  move(onProgress: (p: MoveProgress) => void): Promise<Moved>;
  stop(): Promise<void>;
  /** "Not now", or the computer kept what it has: the pairing is undone on both. */
  undo(): Promise<void>;
  /** What becomes of the memory put aside here: removed now, or kept for 30 days. */
  putAside(remove: boolean): Promise<void>;
  /** A move is part way: this phone still holds its memory and a pairing for it. */
  pending(): Promise<boolean>;
}

/** How long a phone waiting for the person on the computer waits before it asks again. */
const ASK_AGAIN_MS = 5000;

const errorOf = (e: unknown): MoveError => (typeof e === "string" ? (e as MoveError) : "failed");

/** What the phone says for each way a move ends with nothing changed. */
const ENDED: Partial<Record<MoveError, MoveEnded>> = {
  notWhole: "notWhole",
  computerOutdated: "outdated",
  noRoom: "noRoom",
  ranOutOfRoom: "ranOut",
  notOpened: "notOpened",
  stoppedThere: "stoppedThere",
};

/** How a move that did not finish ends on screen: none when the person stopped it. */
export function endedFor(e: unknown): MoveEnded | null {
  const why = errorOf(e);
  return why === "stopped" ? null : (ENDED[why] ?? "notWhole");
}

/** The move through the app's own commands. */
export const nativeMoveApi: MoveApi = {
  computer: async () => {
    const state = await invoke<{ kind: string; hello?: { computerName: string } }>("connect");
    return state.hello?.computerName ?? "";
  },
  holds: async () => {
    const state = await invoke<{ kind: string; hello?: { memoryHere?: boolean } }>("connect");
    return state.hello?.memoryHere === true;
  },
  facts: () => invoke<MoveFacts>("move_facts"),
  offer: () => invoke<MoveOffered>("move_offer").catch((e) => Promise.reject(errorOf(e))),
  move: async (onProgress) => {
    const stop = await listen<MoveProgress>("move-progress", (e) => onProgress(e.payload));
    try {
      return await invoke<Moved>("move_memory");
    } catch (e) {
      throw errorOf(e);
    } finally {
      stop();
    }
  },
  stop: () => invoke<void>("move_stop"),
  undo: () => invoke<void>("move_undo"),
  putAside: (remove) => invoke<void>("move_put_aside", { remove }),
  pending: () => invoke<boolean>("move_pending").then(Boolean, () => false),
};

type Step =
  | { name: "found" }
  | { name: "waiting" }
  | { name: "size" }
  | { name: "moving"; bring: boolean }
  | { name: "failed"; why: MoveEnded; bring: boolean }
  | { name: "moved"; items: number; brought: BroughtCounts | null; busy: boolean }
  | { name: "used"; busy: boolean };

/**
 * Moving this phone's memory to the computer it has just paired with: the question, what moves,
 * moving, and moved. The offer goes to the computer as soon as the question shows, so the person
 * there is asked while this one reads. A computer that already holds a memory asks the person
 * there how this phone joins it, and this one waits: its things are brought in, or the
 * computer's memory is used and nothing goes. `onExit` returns to the phone alone (nothing
 * changed, the pairing undone); `onDone` follows the computer once it is decided.
 */
export function MoveFlow({
  api,
  onExit,
  onDone,
}: {
  api: MoveApi;
  onExit(): void;
  onDone(): void;
}) {
  const [current, setStep] = useState<Step>({ name: "found" });
  const step = useRef(current);
  step.current = current;
  const [computer, setComputer] = useState("");
  const [facts, setFacts] = useState<MoveFacts | null>(null);
  const [progress, setProgress] = useState<MoveProgress | null>(null);
  const offer = useRef<Promise<MoveOffered> | null>(null);
  const leaving = useRef(false);

  const exit = (undo: boolean) => {
    if (leaving.current) return;
    leaving.current = true;
    void (undo ? api.undo() : Promise.resolve()).finally(onExit);
  };

  // Before the person here has moved on: the computer's answer decides the next screen.
  const deciding = () => ["found", "waiting", "size"].includes(step.current.name);

  const answered = (answer: MoveOffered) => {
    if (!deciding()) return;
    if (answer.opened) void move(answer.bring === true);
    else if (answer.useHere) setStep({ name: "used", busy: false });
    else if (answer.bring) void move(true);
    else setStep((s) => (s.name === "waiting" ? { name: "found" } : s));
  };

  const ask = () => {
    const sent = api.offer();
    offer.current = sent;
    sent.then(answered, (e: MoveError) => {
      // The person on the computer kept what it has: the phone stays as it was.
      if (e === "declined") return exit(true);
      // Waiting for the person there: the connection went, or the question timed out. It is
      // asked again, and the phone keeps waiting until they choose or this one cancels. While
      // this person reads, a computer out of reach is asked again too, so a move whose memory
      // already opened there finishes once it answers.
      const again = () =>
        step.current.name === "waiting" || (e === "paused" && deciding());
      if (again())
        setTimeout(() => {
          if (!leaving.current && again() && offer.current === sent) ask();
        }, ASK_AGAIN_MS);
    });
    return sent;
  };

  // The pairing just opened the session: the question and the offer go at once. A memory that
  // already opened there finishes straight away. A computer that holds a memory has this phone
  // wait for the person there, and its answer carries the move on by itself.
  useEffect(() => {
    void api.computer().then(setComputer, () => undefined);
    void api.facts().then(setFacts, () => undefined);
    void api.holds().then(
      (holds) => holds && setStep((s) => (s.name === "found" ? { name: "waiting" } : s)),
      () => undefined,
    );
    ask();
  }, []);

  /** The computer's answer, asked again while it is out of reach: the move is paused till then. */
  const reached = async (): Promise<MoveOffered> => {
    for (;;) {
      try {
        return await (offer.current ?? ask());
      } catch (e) {
        if (errorOf(e) !== "paused") throw e;
        offer.current = null;
        setProgress({ phase: "paused", done: 0, total: 0 });
        await new Promise((r) => setTimeout(r, ASK_AGAIN_MS));
        if (leaving.current) throw "stopped";
      }
    }
  };

  const move = async (asked: boolean) => {
    let bring = asked;
    setStep({ name: "moving", bring });
    setProgress(null);
    try {
      const answer = await reached();
      // The person there chose the computer's memory: what was here is already put aside.
      if (answer.useHere) return setStep({ name: "used", busy: false });
      if (answer.bring && !bring) {
        bring = true;
        setStep({ name: "moving", bring });
      }
      const moved = await api.move(setProgress);
      setStep({ name: "moved", items: moved.items, brought: moved.brought ?? null, busy: false });
    } catch (e) {
      const why = errorOf(e);
      if (why === "declined" || why === "stopped") return exit(why === "declined");
      setStep({ name: "failed", why: ENDED[why] ?? "notWhole", bring });
    }
  };

  const keepOrRemove = (remove: boolean) => {
    if (current.name !== "moved" && current.name !== "used") return;
    setStep({ ...current, busy: true });
    void api.putAside(remove).finally(onDone);
  };

  switch (current.name) {
    case "found":
      return (
        <MoveFound
          computer={computer}
          onContinue={() => setStep({ name: "size" })}
          onNotNow={() => exit(true)}
        />
      );
    case "waiting":
      return <MoveWaiting computer={computer} onCancel={() => exit(true)} />;
    case "size":
      return facts ? (
        <MoveSize
          facts={facts}
          onMove={() => void move(false)}
          onBack={() => setStep({ name: "found" })}
        />
      ) : null;
    case "moving":
      return (
        <MoveMoving
          computer={computer}
          bring={current.bring}
          progress={progress}
          onStop={() => {
            leaving.current = true;
            void api.stop().finally(onExit);
          }}
        />
      );
    case "failed":
      return (
        <MoveFailed
          computer={computer}
          why={current.why}
          onTryAgain={() => {
            offer.current = null;
            void move(current.bring);
          }}
          onBack={() => exit(true)}
        />
      );
    case "moved":
      return (
        <MoveMoved
          computer={computer}
          items={current.items}
          brought={current.brought}
          busy={current.busy}
          onDone={keepOrRemove}
        />
      );
    case "used":
      return <MoveUsed computer={computer} busy={current.busy} onDone={keepOrRemove} />;
  }
}

/** The memory a move put aside on this phone, as This phone shows it. */
export interface MoveAside {
  /** Its things are on the computer: it can only be removed. */
  onComputer: boolean;
  keepUntil: string | null;
}

/** What This phone can do with the memory a move put aside, once the phone is a companion. */
export interface AsideApi {
  aside(): Promise<MoveAside | null>;
  /** "Bring its things to {computer}…": rejects with a `MoveError`. */
  bring(onProgress: (p: MoveProgress) => void): Promise<Moved>;
  stop(): Promise<void>;
  /** Removed now, or kept as it is. */
  putAside(remove: boolean): Promise<void>;
}

/** What was put aside, through the app's own commands. */
export const nativeAsideApi: AsideApi = {
  aside: () => invoke<MoveAside | null>("move_aside").catch(() => null),
  bring: async (onProgress) => {
    const stop = await listen<MoveProgress>("move-progress", (e) => onProgress(e.payload));
    try {
      return await invoke<Moved>("move_bring_aside");
    } catch (e) {
      throw errorOf(e);
    } finally {
      stop();
    }
  },
  stop: () => invoke<void>("move_bring_stop"),
  putAside: (remove) => invoke<void>("move_put_aside", { remove }),
};
