import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { MemoryCounts } from "../../../engine/bindings";
import {
  type MoveFacts,
  MoveFailed,
  MoveFound,
  MoveMoved,
  MoveMoving,
  type MoveProgress,
  MoveSize,
} from "./MoveScreens";

/** Why a move did not finish. Nothing changed on the phone in any of them. */
export type MoveError =
  | "paused"
  | "notWhole"
  | "computerOutdated"
  | "declined"
  | "stopped"
  | "full"
  | "failed";

/** The phone's way to move its memory to the computer it has just paired with. */
export interface MoveApi {
  /** The computer's name, from the session the pairing opened. */
  computer(): Promise<string>;
  facts(): Promise<MoveFacts>;
  /** Offers the memory and waits for the person on the computer; answers what it holds. */
  offer(): Promise<MemoryCounts | null>;
  /** "Move my memory": answers how many things arrived. Rejects with a `MoveError`. */
  move(onProgress: (p: MoveProgress) => void): Promise<{ items: number }>;
  stop(): Promise<void>;
  /** "Not now", or the computer kept what it has: the pairing is undone on both. */
  undo(): Promise<void>;
  /** What becomes of the memory put aside here: removed now, or kept for 30 days. */
  putAside(remove: boolean): Promise<void>;
}

const errorOf = (e: unknown): MoveError => (typeof e === "string" ? (e as MoveError) : "failed");

/** The move through the app's own commands. */
export const nativeMoveApi: MoveApi = {
  computer: async () => {
    const state = await invoke<{ kind: string; hello?: { computerName: string } }>("connect");
    return state.hello?.computerName ?? "";
  },
  facts: () => invoke<MoveFacts>("move_facts"),
  offer: () => invoke<MemoryCounts | null>("move_offer").catch((e) => Promise.reject(errorOf(e))),
  move: async (onProgress) => {
    const stop = await listen<MoveProgress>("move-progress", (e) => onProgress(e.payload));
    try {
      return await invoke<{ items: number }>("move_memory");
    } catch (e) {
      throw errorOf(e);
    } finally {
      stop();
    }
  },
  stop: () => invoke<void>("move_stop"),
  undo: () => invoke<void>("move_undo"),
  putAside: (remove) => invoke<void>("move_put_aside", { remove }),
};

type Step =
  | { name: "found" }
  | { name: "size" }
  | { name: "moving" }
  | { name: "failed"; outdated: boolean }
  | { name: "moved"; items: number; busy: boolean };

/**
 * Moving this phone's memory to the computer it has just paired with: the question, what moves,
 * moving, and moved. The offer goes to the computer as soon as the question shows, so the person
 * there is asked while this one reads. `onExit` returns to the phone alone (nothing changed, the
 * pairing undone); `onDone` follows the computer once the memory is there.
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
  const [step, setStep] = useState<Step>({ name: "found" });
  const [computer, setComputer] = useState("");
  const [facts, setFacts] = useState<MoveFacts | null>(null);
  const [progress, setProgress] = useState<MoveProgress | null>(null);
  const offer = useRef<Promise<MemoryCounts | null> | null>(null);
  const leaving = useRef(false);

  const exit = (undo: boolean) => {
    if (leaving.current) return;
    leaving.current = true;
    void (undo ? api.undo() : Promise.resolve()).finally(onExit);
  };

  const ask = () => {
    const sent = api.offer();
    offer.current = sent;
    // The person on the computer kept what it has: the phone stays as it was.
    sent.catch((e: MoveError) => {
      if (e === "declined") exit(true);
    });
    return sent;
  };

  // The pairing just opened the session: the question and the offer go at once.
  useEffect(() => {
    void api.computer().then(setComputer, () => undefined);
    void api.facts().then(setFacts, () => undefined);
    ask();
  }, []);

  const move = async () => {
    setStep({ name: "moving" });
    setProgress(null);
    try {
      await (offer.current ?? ask());
      const moved = await api.move(setProgress);
      setStep({ name: "moved", items: moved.items, busy: false });
    } catch (e) {
      const why = errorOf(e);
      if (why === "declined" || why === "stopped") return exit(why === "declined");
      setStep({ name: "failed", outdated: why === "computerOutdated" });
    }
  };

  switch (step.name) {
    case "found":
      return (
        <MoveFound
          computer={computer}
          onContinue={() => setStep({ name: "size" })}
          onNotNow={() => exit(true)}
        />
      );
    case "size":
      return facts ? (
        <MoveSize
          facts={facts}
          onMove={() => void move()}
          onBack={() => setStep({ name: "found" })}
        />
      ) : null;
    case "moving":
      return (
        <MoveMoving
          computer={computer}
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
          outdated={step.outdated}
          onTryAgain={() => {
            offer.current = null;
            void move();
          }}
          onBack={() => exit(true)}
        />
      );
    case "moved":
      return (
        <MoveMoved
          computer={computer}
          items={step.items}
          busy={step.busy}
          onDone={(remove) => {
            setStep({ ...step, busy: true });
            void api.putAside(remove).finally(onDone);
          }}
        />
      );
  }
}
