/** Which computer this window is on, from what the web view says about itself. */
export type ComputerKind = "mac" | "windows" | "linux";

export function computerKind(): ComputerKind {
  const text = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Windows/i.test(text)) return "windows";
  if (/Mac OS X|Macintosh/i.test(text)) return "mac";
  return "linux";
}

/**
 * Where Most private is not offered yet. Windows waits for proof that the Tailscale part runs there
 * in the foreground; when that is shown, remove it from this list.
 */
const MOST_PRIVATE_HIDDEN_ON: readonly ComputerKind[] = ["windows"];

export function mostPrivateAvailable(kind: ComputerKind = computerKind()): boolean {
  return !MOST_PRIVATE_HIDDEN_ON.includes(kind);
}

/** Whether the system may ask if the Tailscale part can accept connections. */
export function mayAskAboutFirewall(kind: ComputerKind = computerKind()): boolean {
  return kind === "mac" || kind === "windows";
}
