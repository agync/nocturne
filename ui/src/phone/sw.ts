// The phone app's service worker. It caches the shell, its own scripts, styles, fonts and icons,
// so the app opens with no connection, and nothing else. It never sees `/s`, never caches a
// reply from the computer and holds no key, id or counter.

import { cacheable, routeFor, STATIC_FILES } from "./swPolicy";

interface Waitable {
  waitUntil(work: Promise<unknown>): void;
}
interface FetchEventLike extends Waitable {
  request: Request;
  respondWith(response: Promise<Response> | Response): void;
}
interface MessageEventLike {
  data: unknown;
}
interface Scope {
  addEventListener(type: "install" | "activate", fn: (e: Waitable) => void): void;
  addEventListener(type: "fetch", fn: (e: FetchEventLike) => void): void;
  addEventListener(type: "message", fn: (e: MessageEventLike) => void): void;
  skipWaiting(): Promise<void>;
  clients: { claim(): Promise<void> };
  location: { origin: string };
}

const sw = self as unknown as Scope;

/** Filled in at build time with the files of this build; a development build has none. */
const SHELL: { version: string; files: string[] } = (() => {
  try {
    return JSON.parse("__SHELL_JSON__") as { version: string; files: string[] };
  } catch {
    return { version: "development", files: [] };
  }
})();

const PREFIX = "nocturne-phone-";
const CACHE = `${PREFIX}${SHELL.version}`;

sw.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) =>
        cache.addAll(["/", ...STATIC_FILES, ...SHELL.files.map((file) => `/${file}`)]),
      ),
  );
});

sw.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names.filter((n) => n.startsWith(PREFIX) && n !== CACHE).map((n) => caches.delete(n)),
        ),
      )
      .then(() => sw.clients.claim()),
  );
});

sw.addEventListener("message", (event) => {
  if ((event.data as { type?: string } | null)?.type === "skip-waiting") void sw.skipWaiting();
});

sw.addEventListener("fetch", (event) => {
  const request = event.request;
  const route = routeFor(
    new URL(request.url),
    request.method,
    sw.location.origin,
    request.mode === "navigate",
  );
  if (route === "network") return;
  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      const hit = await cache.match(route === "shell" ? "/" : request);
      if (hit) return hit;
      const response = await fetch(request);
      if (cacheable(response)) await cache.put(route === "shell" ? "/" : request, response.clone());
      return response;
    })(),
  );
});
