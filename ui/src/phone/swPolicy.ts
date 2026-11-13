// What the service worker may touch: the app's own shell and files, nothing else. The sealed
// endpoint `/s` is never cached, never answered from a cache and never intercepted, and neither
// is anything that is not a plain GET to this origin.

export type Route = "network" | "shell" | "asset";

/** Pages the computer serves as the one app shell. */
export const SHELL_PATHS: ReadonlySet<string> = new Set(["/", "/p", "/r"]);
/** Files served from the public folder. */
export const STATIC_FILES: readonly string[] = [
  "/manifest.webmanifest",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
  "/icons/icon-maskable-512.png",
  "/icons/apple-touch-icon.png",
  "/icons/icon.svg",
];

export function routeFor(url: URL, method: string, origin: string, isNavigation: boolean): Route {
  if (method !== "GET") return "network";
  if (url.origin !== origin) return "network";
  if (url.pathname === "/s" || url.pathname.startsWith("/s/")) return "network";
  if (isNavigation && SHELL_PATHS.has(url.pathname)) return "shell";
  if (url.pathname.startsWith("/assets/") || STATIC_FILES.includes(url.pathname)) return "asset";
  return "network";
}

/** Only a complete, same-origin success is worth keeping. */
export function cacheable(response: { ok: boolean; type: string; status: number }): boolean {
  return response.ok && response.status === 200 && response.type === "basic";
}
