import { describe, expect, it } from "vitest";
import { findRoute, routes } from "./routes";

describe("route table", () => {
  it("matches every route of the plan to its folder", () => {
    const table: Record<string, string> = {
      "/today": "today",
      "/pages/2026-11-12": "today",
      "/ask": "ask",
      "/needs-you": "needs-you",
      "/recent": "recent",
      "/quick-capture": "capture",
      "/memories": "memories",
      "/memories/ent-leo": "memories",
      "/items/i1": "memories",
      "/fading": "fading",
      "/pages": "past-pages",
      "/setup": "setup",
      "/privacy": "privacy",
      "/settings": "settings",
      "/settings/advanced": "settings",
      "/settings/away": "away",
      "/settings/away/setup": "away",
      "/settings/away/add": "away",
      "/settings/away/phones": "away",
      "/settings/away/activity": "away",
      "/listen/2026-11-13": "editions",
      "/print/2026-11-13": "editions",
    };
    for (const [path, folder] of Object.entries(table)) {
      expect(findRoute(path)?.folder, path).toBe(folder);
    }
  });

  it("draws quick capture, setup and print without the shell", () => {
    const bare = routes.filter((r) => !r.shell).map((r) => r.path);
    expect(bare.sort()).toEqual(["/print/:date", "/quick-capture", "/setup"]);
  });

  it("does not match unknown paths", () => {
    expect(findRoute("/nowhere")).toBeUndefined();
    expect(findRoute("/pages/a/b")).toBeUndefined();
  });
});

describe("routes with a query", () => {
  it("finds the screen whatever follows the ?", () => {
    expect(findRoute("/memories?q=bakery")?.folder).toBe("memories");
    expect(findRoute("/fading?show=letGo")?.folder).toBe("fading");
    expect(findRoute("/pages/2026-11-12?x=1")?.folder).toBe("today");
    expect(findRoute("/nowhere?q=1")).toBeUndefined();
  });
});
