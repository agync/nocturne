import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { EngineClient } from "../engine/client";
import { createMockEngine } from "../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../test/render";
import { askRequests } from "./ask-request";

beforeEach(() => {
  setViewport(1280);
});
afterEach(() => {
  window.location.hash = "";
  askRequests.take();
});

const SCREENS: Array<[string, string]> = [
  ["#/today", "Two things need you today."],
  ["#/pages/2026-11-12", "One thing needs you today."],
  ["#/needs-you", "Needs you"],
  ["#/ask", "Ask"],
  ["#/recent", "Recently kept"],
  ["#/quick-capture", "Keep something"],
  ["#/memories", "What Nocturne remembers"],
  ["#/memories/ent-leo", "A person, place or thing"],
  ["#/items/item-letter", "Something you kept"],
  ["#/fading", "Fading & let go"],
  ["#/pages", "Past pages"],
  ["#/setup", "Getting ready"],
  ["#/privacy", "Privacy"],
  ["#/settings", "Settings"],
  ["#/settings/advanced", "Advanced details"],
  ["#/listen/2026-11-13", "Listen"],
  ["#/print/2026-11-13", "Print"],
];

describe("routes", () => {
  for (const [hash, heading] of SCREENS) {
    it(`${hash} renders its h1`, async () => {
      renderApp({ hash });
      expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    });
  }

  it("opens on today’s page", async () => {
    renderApp({ hash: "" });
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/today");
  });

  it("says kindly when a page is not here", async () => {
    renderApp({ hash: "#/nowhere" });
    expect(
      await screen.findByRole("heading", { level: 1, name: "That page isn’t here" }),
    ).toBeInTheDocument();
  });

  it("names the window after the screen", async () => {
    renderApp({ hash: "#/privacy" });
    await screen.findByRole("heading", { level: 1, name: "Privacy" });
    expect(document.title).toBe("Privacy · Nocturne");
  });

  it("has no shell on quick capture, setup and print", async () => {
    renderApp({ hash: "#/setup" });
    await screen.findByRole("heading", { level: 1 });
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Keep something, or ask" })).toBeNull();
  });
});

describe("rail", () => {
  it("shows the Needs you count as plain Ember text, not a badge", async () => {
    renderApp();
    const link = await screen.findByRole("link", { name: /Needs you/ });
    const count = await within(link).findByText("2");
    expect(count.tagName).toBe("SPAN");
    expect(count).not.toHaveAttribute("role");
    expect(count.querySelector("*")).toBeNull();
    expect(link).toHaveAccessibleName("Needs you 2");
  });

  it("marks the current screen and the day on Today", async () => {
    renderApp();
    const today = await screen.findByRole("link", { name: /^Today/ });
    expect(today).toHaveAttribute("aria-current", "page");
    expect(await within(today).findByText("Fri")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Settings" })).not.toHaveAttribute("aria-current");
  });

  it("lists the whole rail in order with Privacy and Settings below", async () => {
    renderApp();
    const nav = await screen.findByRole("navigation", { name: "Main" });
    const names = within(nav)
      .getAllByRole("link")
      .map((a) => a.textContent?.trim());
    expect(names.slice(0, 6).map((n) => n?.replace(/\d|Fri/g, "").trim())).toEqual([
      "Today",
      "Needs you",
      "What Nocturne remembers",
      "Recently kept",
      "Fading & let go",
      "Past pages",
    ]);
    expect(names.slice(6)).toEqual(["Privacy", "Settings"]);
  });

  it("shows the state and what was sent to the internet, from the live status", async () => {
    renderApp();
    const status = await screen.findByRole("region", { name: "Nocturne status" });
    expect(within(status).getByText("Resting")).toBeInTheDocument();
    expect(within(status).getByText("Sent to the internet")).toBeInTheDocument();
    expect(within(status).getByText("Nothing")).toBeInTheDocument();
    expect(status.querySelector('[data-state="resting"]')).not.toBeNull();
  });

  it("shows bytes when something was sent, and says when it cannot measure", async () => {
    const engine = createMockEngine();
    const real = engine.getStatus.bind(engine);
    engine.getStatus = async () => ({ ...(await real()), sentToInternetBytes: 84_000_000 });
    const first = renderApp({ engine });
    expect(await screen.findByText("84 MB")).toBeInTheDocument();
    first.unmount();
    const engine2 = createMockEngine();
    const real2 = engine2.getStatus.bind(engine2);
    engine2.getStatus = async () => ({ ...(await real2()), sentMeasured: false });
    renderApp({ engine: engine2 });
    expect(await screen.findByText("Not measured on this computer")).toBeInTheDocument();
  });
});

describe("narrow window", () => {
  beforeEach(() => setViewport(400));

  it("replaces the rail with Menu and docks the bar at the bottom", async () => {
    const { container } = renderApp();
    const menu = await screen.findByRole("button", { name: "Menu" });
    expect(menu).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    const bar = container.querySelector('[data-placement="bottom"]');
    expect(bar).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Keep something, or ask" })).toHaveAttribute(
      "placeholder",
      "Keep or ask…",
    );
    expect(container.querySelector('[data-placement="top"]')).toBeNull();
  });

  it("opens the rail as a menu, closes on a link, and returns focus on Escape", async () => {
    renderApp();
    const menu = await screen.findByRole("button", { name: "Menu" });
    await userEvent.click(menu);
    const dialog = await screen.findByRole("dialog", { name: "Menu" });
    expect(within(dialog).getByRole("link", { name: "Privacy" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(menu).toHaveFocus();

    await userEvent.click(menu);
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("link", { name: "Privacy" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(window.location.hash).toBe("#/privacy"));
  });

  it("goes back to the wide layout when the window grows", async () => {
    renderApp();
    await screen.findByRole("button", { name: "Menu" });
    act(() => setViewport(1280));
    expect(await screen.findByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Menu" })).toBeNull();
  });
});

describe("wide window", () => {
  it("puts the bar at the top with the drop hint, and the kept-today column beside the page", async () => {
    const { container } = renderApp();
    expect(
      await screen.findByRole("textbox", { name: "Keep something, or ask" }),
    ).toBeInTheDocument();
    expect(container.querySelector('[data-placement="top"]')).not.toBeNull();
    expect(screen.getByText("or drop files anywhere")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Menu" })).toBeNull();
    expect(await screen.findByRole("complementary", { name: "Recently kept" })).toBeInTheDocument();
  });

  it("autofocuses the bar when the window opens", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    expect(bar).toHaveFocus();
  });

  it("cycles focus with F6: bar, page, rail, bar", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    expect(bar).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByRole("main")).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByRole("link", { name: /^Today/ })).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(bar).toHaveFocus();
    await userEvent.keyboard("{Shift>}{F6}{/Shift}");
    expect(screen.getByRole("link", { name: /^Today/ })).toHaveFocus();
  });
});

describe("keeping and asking from the bar", () => {
  it("keeps a note, says so, and lists it", async () => {
    const { engine } = renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await userEvent.type(bar, "Ask Sam about Saturday{Enter}");
    expect(await screen.findByText("Kept.")).toBeInTheDocument();
    expect(screen.getByText("I’ll read it tonight.")).toBeInTheDocument();
    const items = await engine.listItems({ since: null, status: null, limit: 1 });
    expect(items[0]?.title).toBe("Ask Sam about Saturday");
    expect(bar).toHaveValue("");
  });

  it("says kindly when it was already kept", async () => {
    const { engine } = renderApp();
    await engine.keepText("Bin day is Tuesday", "paste");
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await userEvent.type(bar, "Bin day is Tuesday{Enter}");
    expect(await screen.findByText(/You already kept this note on/)).toBeInTheDocument();
  });

  it("sends a question to the Ask screen", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await userEvent.type(bar, "When is the dentist?{Enter}");
    await waitFor(() => expect(window.location.hash).toBe("#/ask"));
    expect(askRequests.peek()?.question).toBe("When is the dentist?");
  });
});

describe("notices", () => {
  it("shows what the engine says needs a word, with the one action that can be done", async () => {
    const engine = createMockEngine();
    const real = engine.getStatus.bind(engine);
    engine.getStatus = async () => ({
      ...(await real()),
      notices: [
        { id: "n1", code: "restarts", itemId: null, etaSeconds: null, bytes: null },
        { id: "n2", code: "tierDropped", itemId: null, etaSeconds: null, bytes: null },
      ],
    });
    renderApp({ engine });
    const region = await screen.findByRole("region", { name: "Things to know" });
    expect(within(region).getAllByRole("listitem")).toHaveLength(2);
    await userEvent.click(within(region).getByRole("button", { name: "Show details" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/advanced"));
    expect(within(region).queryByRole("button", { name: "OK" })).toBeNull();
  });
});

describe("drop anywhere", () => {
  it("says what will be kept while files are over the window, then keeps them", async () => {
    const platform = fakePlatform();
    const engine: EngineClient = createMockEngine();
    renderApp({ engine, platform });
    await screen.findByRole("textbox", { name: "Keep something, or ask" });
    act(() => platform.hover(4));
    expect(screen.getByText("Drop to keep 4 things")).toBeInTheDocument();
    expect(
      screen.getByText("Nocturne keeps the originals exactly as they are."),
    ).toBeInTheDocument();
    act(() => platform.drop(["a.jpg", "b.pdf"]));
    act(() => platform.hover(null));
    expect(screen.queryByText("Drop to keep 4 things")).toBeNull();
    expect(await screen.findByText(/Kept 2 files/)).toBeInTheDocument();
    const items = await engine.listItems({ since: null, status: null, limit: 2 });
    expect(items.map((i) => i.title).sort()).toEqual(["a.jpg", "b.pdf"]);
  });
});

describe("theme", () => {
  it("follows the system theme and can be forced by the setting", async () => {
    const platform = fakePlatform("dark");
    const engine = createMockEngine();
    renderApp({ engine, platform });
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    act(() => platform.theme("light"));
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "light"));
  });
});
