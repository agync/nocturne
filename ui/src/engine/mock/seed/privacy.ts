import type { PrivacyView } from "../../bindings";

/** The privacy page's fixed facts; the live parts (bytes sent, lock, update checks) come from the engine. */
export function privacySeed(
  now: Date,
): Pick<PrivacyView, "checkedAt" | "ledger" | "keptBytes" | "mindBytes" | "dataDir"> {
  return {
    checkedAt: new Date(now.getTime() - 2000).toISOString(),
    ledger: [{ kind: "setup", label: "Nocturne’s mind", bytes: 8.4e9, at: now.toISOString() }],
    keptBytes: 4.3e9,
    mindBytes: 8.4e9,
    dataDir: "Documents/Nocturne",
  };
}
