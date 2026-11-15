import { useEffect, useRef, useState } from "react";
import { formatBytes, formatEta } from "../../copy/format";
import { t } from "../../copy/t";
import type { BackupProgress } from "../../engine/bindings";

export interface Sample {
  phase: BackupProgress["phase"];
  done: number;
  at: number;
}

/** A rate needs a stretch of time behind it before the guess is worth saying. */
const HONEST_AFTER_MS = 1000;

/**
 * Seconds left, from how much moved since the first sample of this phase, or null when the
 * sample is too young or nothing moved. Never a guess from thin air: a screen shows the plain
 * count until this answers.
 */
export function etaSeconds(progress: BackupProgress, first: Sample, now: number): number | null {
  const elapsed = (now - first.at) / 1000;
  const moved = progress.done - first.done;
  const left = progress.total - progress.done;
  if (elapsed <= 0 || moved <= 0 || left <= 0 || now - first.at < HONEST_AFTER_MS) return null;
  return left / (moved / elapsed);
}

/** The words for one progress step: bytes, and the time left only when it can be said honestly. */
export function progressDetail(progress: BackupProgress, eta: number | null): string {
  const done = formatBytes(progress.done);
  const total = formatBytes(progress.total);
  return eta === null
    ? t("backup.progress.plain", { done, total })
    : t("backup.progress.eta", { done, total, eta: formatEta(eta) });
}

/** Follows one run's progress, keeping the first sample of each phase for the rate. */
export function useProgressEta(progress: BackupProgress | null): number | null {
  const [eta, setEta] = useState<number | null>(null);
  const sample = useRef<Sample | null>(null);

  useEffect(() => {
    if (!progress) {
      sample.current = null;
      setEta(null);
      return;
    }
    const now = Date.now();
    const first = sample.current;
    if (!first || first.phase !== progress.phase || progress.done < first.done) {
      sample.current = { phase: progress.phase, done: progress.done, at: now };
      setEta(null);
      return;
    }
    setEta(etaSeconds(progress, first, now));
  }, [progress]);

  return eta;
}
