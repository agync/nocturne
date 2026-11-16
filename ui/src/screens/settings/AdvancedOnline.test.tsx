import { screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OnlineAdvanced } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const SECRET = "sk-test-secret-0123456789";

const advanced = (over: Partial<OnlineAdvanced> = {}): OnlineAdvanced => ({
  state: "on since 2 Nov · not paused",
  route: "127.0.0.1:8020",
  baseUrl: "https://api.example.com/v1",
  host: "api.example.com",
  model: "llama-3.3-70b-instruct",
  maxTokens: 600,
  keyStorage: "keychain",
  lastAt: "2026-11-13T09:14:00Z",
  lastStatus: 200,
  lastSeconds: 1.2,
  lastBytesUp: 1400,
  lastBytesDown: 600,
  costReported: false,
  sendsKept: 38,
  questionsThisMonth: 38,
  ...over,
});

async function open(online: OnlineAdvanced | null, seedKey = false) {
  const engine = createMockEngine();
  if (seedKey) {
    // The key goes in through the form, as a person would enter it.
    const form = { baseUrl: "https://api.example.com/v1", key: SECRET, model: "m" };
    await engine.checkOnline(form);
    await engine.turnOnOnline({
      phones: false,
      sendSensitive: false,
      limit: { kind: "questions", questions: 200, usd: null },
    });
  }
  const real = engine.getAdvanced.bind(engine);
  vi.spyOn(engine, "getAdvanced").mockImplementation(async () => ({ ...(await real()), online }));
  renderApp({ hash: "#/settings/advanced", engine });
  await screen.findByText("tier");
}

describe("the online block of the advanced details", () => {
  it("is not there when Online answers was never set up", async () => {
    await open(null);
    expect(screen.queryByRole("heading", { name: "Online answers" })).toBeNull();
    expect(screen.queryByText("cloud providers")).toBeNull();
  });

  it("advanced.online_block_has_no_key: shows the route, the limits and where the key lives, never the key", async () => {
    await open(advanced(), true);
    const block = await screen.findByRole("region", { name: "Online answers" });
    const rows = new Map(
      within(block)
        .getAllByRole("term")
        .map((dt) => [dt.textContent, dt.nextElementSibling?.textContent] as const),
    );
    expect(rows.get("state")).toBe("on since 2 Nov · not paused · 38 questions this month");
    expect(rows.get("sent by")).toBe(
      "engine cloud providers (experimental) · 127.0.0.1:8020 → https://api.example.com/v1",
    );
    expect(rows.get("cloud providers")).toBe("1 · api.example.com · no others");
    expect(rows.get("model")).toBe(
      "llama-3.3-70b-instruct (as entered) · at most 600 tokens out · no tools",
    );
    expect(rows.get("key")).toBe(
      "system keychain · given to the engine at launch · never logged or exported",
    );
    expect(rows.get("retention")).toBe("set by your provider · not known to Nocturne");
    expect(rows.get("last request")).toBe(
      "09:14 · 200 · answered in 1.2 s · 1.4\u00a0kB up · 600\u00a0B down",
    );
    expect(rows.get("cost")).toBe("not reported by the provider · limit counts questions");
    expect(rows.get("sent log")).toBe("38 entries · kept 90 days");
    expect(document.body.textContent).not.toContain(SECRET);
  });

  it("says where the key is kept when the keychain is not available", async () => {
    await open(advanced({ keyStorage: "file" }));
    expect(
      await screen.findByText(/owner-only file · given to the engine at launch/),
    ).toBeInTheDocument();
  });

  it("says there was no request yet, and a cost the provider reports", async () => {
    await open(
      advanced({
        lastAt: null,
        lastStatus: null,
        lastSeconds: null,
        lastBytesUp: null,
        lastBytesDown: null,
        costReported: true,
      }),
    );
    expect(await screen.findByText("none yet")).toBeInTheDocument();
    expect(screen.getByText("reported by the provider")).toBeInTheDocument();
  });
});
