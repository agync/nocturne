/**
 * The token of the development preview or the server. Each prints a link that ends in
 * `#token=...`: the fragment never reaches the server in a URL. It is sent as a bearer header, and
 * the fragment is removed from the address so the router and the history never see it. The
 * preview's token changes at every launch and is kept for this tab only; the server's lasts until
 * it is rotated, so it is kept in this browser until then.
 */
const SERVER = import.meta.env.MODE === "server";
const KEY = SERVER ? "nocturne.server.token" : "nocturne.preview.token";

function store(): Storage | null {
  try {
    return (SERVER ? globalThis.localStorage : globalThis.sessionStorage) ?? null;
  } catch {
    return null;
  }
}

/** Take the token out of the address, if it carries one, and remember it for this tab. */
export function captureToken(): string | null {
  const hash = globalThis.location?.hash ?? "";
  const match = /^#(?:.*&)?token=([0-9a-fA-F]+)(?:&.*)?$/.exec(hash);
  if (match) {
    try {
      store()?.setItem(KEY, match[1] as string);
    } catch {
      // A tab that cannot keep it holds it in memory only.
      memory = match[1] as string;
    }
    globalThis.history?.replaceState(null, "", `${location.pathname}${location.search}#/`);
  }
  return currentToken();
}

let memory: string | null = null;

export function currentToken(): string | null {
  try {
    return store()?.getItem(KEY) ?? memory;
  } catch {
    return memory;
  }
}

export function authHeaders(token: string | null): Record<string, string> {
  return token ? { authorization: `Bearer ${token}` } : {};
}
