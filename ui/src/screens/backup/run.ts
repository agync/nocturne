import { useSyncExternalStore } from "react";
import type { BackupReport } from "../../engine/bindings";
import type { EngineClient } from "../../engine/client";

export type Outcome = { report: BackupReport } | { error: unknown } | null;

export interface BackupAttempt {
  path: string;
  /** Kept only in memory, so a screen that comes back can still try again. */
  passphrase: string;
  outcome: Outcome;
}

let attempt: BackupAttempt | null = null;
const listeners = new Set<() => void>();

function changed(): void {
  for (const listener of [...listeners]) listener();
}

/**
 * One backup at a time, held outside the screen so leaving the screen does not stop it. The
 * passphrase lives in memory only, and nothing here is written anywhere.
 */
export function startBackup(engine: EngineClient, path: string, passphrase: string): BackupAttempt {
  const started: BackupAttempt = { path, passphrase, outcome: null };
  attempt = started;
  changed();
  const settle = (outcome: NonNullable<Outcome>) => {
    // A new object, so every subscriber sees the change. Clearing it first means the answer is
    // dropped, not shown to a screen that has moved on.
    if (attempt !== started) return;
    attempt = { ...started, outcome };
    changed();
  };
  engine.makeBackup(path, passphrase).then(
    (report) => settle({ report }),
    (error: unknown) => settle({ error }),
  );
  return started;
}

export const activeBackup = (): BackupAttempt | null => attempt;

/** Forgets the finished run, and with it the passphrase. */
export function clearBackup(): void {
  if (!attempt) return;
  attempt = null;
  changed();
}

export function subscribeBackup(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function useBackupAttempt(): BackupAttempt | null {
  return useSyncExternalStore(subscribeBackup, activeBackup, () => null);
}
