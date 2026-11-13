import { useSyncExternalStore } from "react";

/** The part of the hash after `?`, as search params: `#/memories?q=bakery` gives `q=bakery`. */
export function hashParams(hash: string = window.location.hash): URLSearchParams {
  const at = hash.indexOf("?");
  return new URLSearchParams(at === -1 ? "" : hash.slice(at + 1));
}

/** The path part of a hash location, without its query. */
export function pathOf(location: string): string {
  const at = location.indexOf("?");
  return at === -1 ? location : location.slice(0, at);
}

function subscribe(cb: () => void): () => void {
  window.addEventListener("hashchange", cb);
  return () => window.removeEventListener("hashchange", cb);
}

/** A query parameter of the current screen, or null. Re-reads whenever the hash changes. */
export function useQueryParam(name: string): string | null {
  return useSyncExternalStore(
    subscribe,
    () => hashParams().get(name),
    () => null,
  );
}

/** `#/memories?q=bakery` for links and navigation. Empty and null values are left out. */
export function hashHref(
  path: string,
  params: Record<string, string | null | undefined> = {},
): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== null && value !== undefined && value !== "") search.set(key, value);
  }
  const text = search.toString();
  return `#${path}${text ? `?${text}` : ""}`;
}

/** Moves to a screen, query included. (wouter's own hash navigate would put the query before the `#`.) */
export function goTo(path: string, params: Record<string, string | null | undefined> = {}): void {
  window.location.hash = hashHref(path, params);
}
