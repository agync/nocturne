// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { routeFor } from "./swPolicy";

const origin = "https://amber-otter-lamp-quiet.trycloudflare.com";
const at = (path: string) => new URL(path, origin);

describe("the service worker's routes", () => {
  it("serves the shell for the app's own pages, and its files from the cache", () => {
    for (const p of ["/", "/p", "/r"]) expect(routeFor(at(p), "GET", origin, true)).toBe("shell");
    expect(routeFor(at("/assets/index-abc.js"), "GET", origin, false)).toBe("asset");
    expect(routeFor(at("/manifest.webmanifest"), "GET", origin, false)).toBe("asset");
  });

  it("sw.never_caches_api: the sealed endpoint is never touched, however it is asked", () => {
    for (const url of ["/s", "/s?sid=00", "/s/anything"]) {
      for (const method of ["GET", "POST", "PUT"]) {
        for (const nav of [true, false])
          expect(routeFor(at(url), method, origin, nav)).toBe("network");
      }
    }
  });

  it("leaves other origins, other paths and every non-GET alone", () => {
    expect(routeFor(new URL("https://elsewhere.example/assets/x.js"), "GET", origin, false)).toBe(
      "network",
    );
    expect(routeFor(at("/api"), "GET", origin, false)).toBe("network");
    expect(routeFor(at("/"), "POST", origin, true)).toBe("network");
    expect(routeFor(at("/p"), "GET", origin, false)).toBe("network");
  });
});

describe("the worker itself", () => {
  afterEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
  });

  it("sw.never_caches_api: a fetch of /s gets no response from the worker and nothing is cached", async () => {
    const handlers: Record<string, (e: unknown) => void> = {};
    const puts: string[] = [];
    const cache = {
      match: async () => undefined,
      put: async (req: Request | string) => void puts.push(typeof req === "string" ? req : req.url),
      addAll: async () => undefined,
    };
    vi.stubGlobal("self", {
      addEventListener: (type: string, fn: (e: unknown) => void) => {
        handlers[type] = fn;
      },
      location: { origin },
      skipWaiting: async () => undefined,
      clients: { claim: async () => undefined },
    });
    vi.stubGlobal("caches", {
      open: async () => cache,
      keys: async () => [],
      delete: async () => true,
    });
    await import("./sw");

    const respond = vi.fn();
    for (const [path, method] of [
      ["/s", "POST"],
      ["/s?sid=ab", "GET"],
    ] as const) {
      handlers.fetch?.({
        request: new Request(`${origin}${path}`, {
          method,
          body: method === "POST" ? "x" : undefined,
        }),
        respondWith: respond,
      });
    }
    expect(respond).not.toHaveBeenCalled();
    expect(puts).toEqual([]);

    handlers.fetch?.({
      request: new Request(`${origin}/assets/app.js`),
      respondWith: respond,
    });
    expect(respond).toHaveBeenCalledOnce();
  });
});
