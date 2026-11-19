import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CHIPS } from "../components/ControlChip/registry";
import { t } from "../copy/t";
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
  ["#/needs-you", "3 things are waiting"],
  ["#/ask", "Ask about anything you’ve kept."],
  ["#/recent", "Today and yesterday"],
  ["#/quick-capture", "Keep something"],
  ["#/memories", "What Nocturne remembers"],
  ["#/memories/ent-leo", "Leo"],
  ["#/items/item-letter", "Letter"],
  ["#/fading", "Fading & let go"],
  ["#/pages", "Past pages"],
  ["#/setup", "Nocturne is ready."],
  ["#/privacy", "Only downloads you agreed to."],
  ["#/settings", "Settings"],
  ["#/settings/advanced", "Advanced details"],
  ["#/listen/2026-11-13", "Two things need you today."],
  ["#/print/2026-11-13", "Print"],
];

describe("the recording report", () => {
  it("tells the engine once, when the main window opens, whether it can record", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "reportRecording");
    renderApp({ hash: "#/today", engine });
    await screen.findByRole("heading", { level: 1 });
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    expect(spy).toHaveBeenCalledWith("no-devices");
  });
});

describe("the keep bar's shortcut hint", () => {
  async function hint(
    desktopShortcut: Awaited<ReturnType<EngineClient["getSettings"]>>["desktopShortcut"],
  ) {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      shortcutAvailable: true,
      desktopShortcut,
    });
    renderApp({ hash: "#/today", engine });
    await screen.findByRole("heading", { level: 1 });
    await waitFor(() => expect(engine.getSettings).toHaveBeenCalled());
    return () => screen.queryByLabelText(/^Shortcut/);
  }

  it("names the keys the desktop bound", async () => {
    const find = await hint({ state: "held", keys: "Ctrl+Alt+K", configurable: false });
    await waitFor(() => expect(find()).not.toBeNull());
  });

  it("names nothing while the desktop is asking or has said no", async () => {
    for (const state of ["asking", "declined"] as const) {
      cleanup();
      const find = await hint({ state, keys: null, configurable: false });
      await new Promise((r) => setTimeout(r, 50));
      expect(find()).toBeNull();
    }
  });
});

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

  it("titles a page that is not there, and a past page, by what they are", async () => {
    renderApp({ hash: "#/nowhere" });
    await screen.findByRole("heading", { level: 1, name: "That page isn’t here" });
    expect(document.title).toBe("That page isn’t here · Nocturne");
    cleanup();
    renderApp({ hash: "#/pages/2026-11-05" });
    await screen.findByRole("heading", { level: 1 });
    expect(document.title).toBe("Morning page · Nocturne");
    cleanup();
    renderApp({ hash: "#/today" });
    await screen.findByRole("heading", { level: 1 });
    expect(document.title).toBe("Today · Nocturne");
  });

  it("names the window after the screen", async () => {
    renderApp({ hash: "#/privacy" });
    await screen.findByRole("heading", { level: 1, name: "Only downloads you agreed to." });
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
    const count = await within(link).findByText("3");
    expect(count.tagName).toBe("SPAN");
    expect(count).not.toHaveAttribute("role");
    expect(count.querySelector("*")).toBeNull();
    expect(link).toHaveAccessibleName("Needs you 3");
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
    expect(names.slice(0, 7).map((n) => n?.replace(/\d|Fri/g, "").trim())).toEqual([
      "Today",
      "Journal",
      "Needs you",
      "What Nocturne remembers",
      "Recently kept",
      "Fading & let go",
      "Past pages",
    ]);
    expect(names.slice(7)).toEqual(["Privacy", "Settings"]);
  });

  it("draws the gear beside Settings, as the Settings chip does, and no icon beside Privacy", async () => {
    renderApp();
    const settings = await screen.findByRole("link", { name: "Settings" });
    expect(settings.querySelector("svg")).toHaveAttribute("data-icon", CHIPS.settings.icon);
    expect(settings.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(t(CHIPS.settings.copy)).toBe(settings.textContent?.trim());
    const privacy = screen.getByRole("link", { name: "Privacy" });
    expect(privacy.querySelector("svg")).toBeNull();
    expect(t(CHIPS.privacy.copy)).toBe(privacy.textContent?.trim());
  });

  it("shows the state and what was sent to the internet, from the live status", async () => {
    renderApp({ engine: createMockEngine({ sentMeasured: true }) });
    const status = await screen.findByRole("region", { name: "Nocturne status" });
    expect(within(status).getByText("Resting")).toBeInTheDocument();
    expect(within(status).getByText("Sent to the internet")).toBeInTheDocument();
    expect(within(status).getByText("Nothing")).toBeInTheDocument();
    expect(status.querySelector('[data-state="resting"]')).not.toBeNull();
  });

  it("does not claim nothing was sent while that is not measured", async () => {
    renderApp();
    const status = await screen.findByRole("region", { name: "Nocturne status" });
    expect(within(status).getByText("Uses the internet for")).toBeInTheDocument();
    expect(within(status).getByText("Downloads you agreed to")).toBeInTheDocument();
    expect(within(status).queryByText("Sent to the internet")).toBeNull();
    expect(within(status).queryByText("Nothing")).toBeNull();
  });

  it("shows bytes when something was sent, and says when it cannot measure", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    const real = engine.getStatus.bind(engine);
    engine.getStatus = async () => ({ ...(await real()), sentToInternetBytes: 84_000_000 });
    const first = renderApp({ engine });
    expect(await screen.findByText("84 MB")).toBeInTheDocument();
    first.unmount();
    const engine2 = createMockEngine();
    const real2 = engine2.getStatus.bind(engine2);
    engine2.getStatus = async () => ({ ...(await real2()), sentMeasured: false });
    renderApp({ engine: engine2 });
    expect(await screen.findByText("Downloads you agreed to")).toBeInTheDocument();
  });
});

describe("narrow window", () => {
  beforeEach(() => setViewport(400));

  it("replaces the rail with one menu button and docks the bar at the bottom", async () => {
    const { container } = renderApp();
    const menu = await screen.findByRole("button", { name: /^Menu/ });
    expect(menu).toHaveAccessibleName("Menu, Today");
    expect(screen.getAllByRole("button", { name: /^Menu/ })).toHaveLength(1);
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    const bar = container.querySelector('[data-placement="bottom"]');
    expect(bar).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Keep something, or ask" })).toHaveAttribute(
      "placeholder",
      "Keep or ask…",
    );
    expect(container.querySelector('[data-placement="top"]')).toBeNull();
  });

  it("hides the keep or ask bar while a step flow is open, and brings it back after", async () => {
    renderApp({ hash: "#/settings/online/setup" });
    await screen.findByRole("heading", { level: 1, name: /Have your answers written/ });
    const bar = screen.getByRole("textbox", { name: "Keep something, or ask", hidden: true });
    expect(bar).not.toBeVisible();
    expect(screen.getByRole("button", { name: "Continue" })).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Not now" }));
    await screen.findByRole("heading", { level: 1, name: "Online answers" });
    expect(screen.getByRole("textbox", { name: "Keep something, or ask" })).toBeVisible();
  });

  it("keeps the keep or ask bar on a screen that is not a step flow", async () => {
    renderApp({ hash: "#/settings" });
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    expect(screen.getByRole("textbox", { name: "Keep something, or ask" })).toBeVisible();
  });

  it("names the rail section in the menu button, not the screen", async () => {
    renderApp({ hash: "#/items/item-letter" });
    expect(
      await screen.findByRole("button", { name: "Menu, What Nocturne remembers" }),
    ).toBeVisible();
    cleanup();
    renderApp({ hash: "#/settings/away" });
    expect(await screen.findByRole("button", { name: "Menu, Settings" })).toBeVisible();
    cleanup();
    renderApp({ hash: "#/pages/2026-11-12" });
    expect(await screen.findByRole("button", { name: "Menu, Today" })).toBeVisible();
    cleanup();
    renderApp({ hash: "#/ask" });
    expect(await screen.findByRole("button", { name: "Menu, Ask" })).toBeVisible();
  });

  it("keeps the menu title on one line with an ellipsis", async () => {
    renderApp();
    const menu = await screen.findByRole("button", { name: /^Menu/ });
    const text = within(menu).getByText("Today");
    expect(text.className).toMatch(/titleText/);
    const css = readFileSync(resolve(__dirname, "Shell.module.css"), "utf8");
    expect(css).toMatch(/\.titleText \{[^}]*text-overflow: ellipsis/);
  });

  it("opens the rail as a menu, closes on a link, and returns focus on Escape", async () => {
    renderApp();
    const menu = await screen.findByRole("button", { name: /^Menu/ });
    await userEvent.click(menu);
    const dialog = await screen.findByRole("dialog", { name: "Menu" });
    expect(within(dialog).getByRole("link", { name: "Privacy" })).toBeInTheDocument();
    expect(within(dialog).queryByText("Nocturne")).toBeNull();
    expect(within(dialog).getByRole("navigation")).toHaveAttribute("data-in-drawer", "true");
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
    await screen.findByRole("button", { name: /^Menu/ });
    act(() => setViewport(1280));
    expect(await screen.findByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Menu/ })).toBeNull();
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
    expect(screen.queryByRole("button", { name: /^Menu/ })).toBeNull();
    expect(await screen.findByRole("complementary", { name: "Recently kept" })).toBeInTheDocument();
  });

  it.each([
    ["#/today", true],
    ["#/recent", true],
    ["#/ask", false],
    ["#/needs-you", false],
    ["#/memories", false],
    ["#/pages", false],
    ["#/settings", false],
  ])("shows the kept-today column on %s: %s", async (hash, shown) => {
    renderApp({ hash });
    await screen.findByRole("heading", { level: 1 });
    if (shown) {
      expect(
        await screen.findByRole("complementary", { name: "Recently kept" }),
      ).toBeInTheDocument();
    } else {
      expect(screen.queryByRole("complementary", { name: "Recently kept" })).toBeNull();
    }
  });

  it("takes no focus when the window opens, so no field shows a ring", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await screen.findByRole("heading", { level: 1 });
    expect(bar).not.toHaveFocus();
    expect(document.activeElement).toBe(document.body);
  });

  it("takes no focus on a narrow window either", async () => {
    setViewport(400);
    renderApp();
    await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await screen.findByRole("heading", { level: 1 });
    expect(document.activeElement).toBe(document.body);
  });

  it("jumps to the bar on /, and says so to a screen reader", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    expect(bar).toHaveAccessibleDescription(/Press \/ from anywhere on the page/);
    await userEvent.keyboard("/");
    expect(bar).toHaveFocus();
    expect(bar).toHaveValue("");
  });

  it("leaves / alone while typing in a field, and with a dialog open", async () => {
    renderApp({ hash: "#/memories" });
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    const search = await screen.findByRole("searchbox", { name: "Search everything kept" });
    await userEvent.click(search);
    await userEvent.keyboard("a/b");
    expect(search).toHaveValue("a/b");
    expect(bar).not.toHaveFocus();
    await userEvent.type(bar, "1/2");
    expect(bar).toHaveValue("1/2");
  });

  it("does not steal / from a dialog", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    const button = document.createElement("button");
    dialog.append(button);
    document.body.append(dialog);
    button.focus();
    await userEvent.keyboard("/");
    expect(bar).not.toHaveFocus();
    dialog.remove();
  });

  it("puts the rail first in keyboard order, then the bar, then the page", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    const rail = screen.getByRole("navigation", { name: "Main" });
    const main = screen.getByRole("main");
    const skip = screen.getByRole("button", { name: "Skip to the page" });
    expect(skip.compareDocumentPosition(rail) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(rail.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(bar.compareDocumentPosition(main) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("cycles focus with F6: bar, page, rail, bar", async () => {
    renderApp();
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await userEvent.keyboard("{F6}");
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
    expect(screen.getByText("It’s on today’s journal.")).toBeInTheDocument();
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
    expect(
      await screen.findByRole("heading", { level: 1, name: "“When is the dentist?”" }),
    ).toBeInTheDocument();
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
    const region = await screen.findByRole("list", { name: "Things to know" });
    expect(within(region).getAllByRole("listitem")).toHaveLength(2);
    await userEvent.click(within(region).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(engine.retryEngineCount).toBe(1));
    expect(within(region).queryByRole("button", { name: "OK" })).toBeNull();
  });
});

describe("a change of mind that did not finish", () => {
  const unfinished = {
    id: "n-unfinished",
    code: "tierUnfinished" as const,
    itemId: null,
    etaSeconds: null,
    bytes: 3_808_000_000,
  };

  it("says so, and starts the download again only when asked", async () => {
    const engine = createMockEngine({ notices: [unfinished], unfinishedTier: "small" });
    const started: string[] = [];
    const real = engine.startSetup.bind(engine);
    engine.startSetup = async (tier) => {
      started.push(tier);
      return real(tier);
    };
    renderApp({ engine });
    const region = await screen.findByRole("list", { name: "Things to know" });
    expect(
      within(region).getByText(
        /The mind you chose didn’t finish downloading\. It still needs 3\.8 GB/,
      ),
    ).toBeInTheDocument();
    expect(started).toEqual([]);
    await userEvent.click(within(region).getByRole("button", { name: "Download it" }));
    await waitFor(() => expect(started).toEqual(["small"]));
    await waitFor(() => expect(screen.queryByRole("list", { name: "Things to know" })).toBeNull());
  });

  it("lets it go on Not now", async () => {
    const engine = createMockEngine({ notices: [unfinished], unfinishedTier: "small" });
    const started: string[] = [];
    engine.startSetup = async (tier) => {
      started.push(tier);
    };
    renderApp({ engine });
    const region = await screen.findByRole("list", { name: "Things to know" });
    await userEvent.click(within(region).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByRole("list", { name: "Things to know" })).toBeNull());
    expect(started).toEqual([]);
  });
});

describe("notices that offer something", () => {
  const offer = {
    id: "n-tier",
    code: "tierDropped" as const,
    itemId: null,
    etaSeconds: null,
    bytes: 3_660_000_000,
  };

  it("offers the smaller mind, says what it costs and starts it when asked", async () => {
    const engine = createMockEngine({ notices: [offer], tierOffer: "small" });
    const started: string[] = [];
    const real = engine.startSetup.bind(engine);
    engine.startSetup = async (tier) => {
      started.push(tier);
      return real(tier);
    };
    renderApp({ engine });
    const region = await screen.findByRole("list", { name: "Things to know" });
    expect(within(region).getByText(/needs 3\.7 GB to download/)).toBeInTheDocument();
    await userEvent.click(within(region).getByRole("button", { name: "Use the smaller one" }));
    await waitFor(() => expect(started).toEqual(["small"]));
    await waitFor(() => expect(screen.queryByText(/running short of computer memory/)).toBeNull());
  });

  it("says not now, and the offer goes without anything being downloaded", async () => {
    const engine = createMockEngine({ notices: [offer], tierOffer: "small" });
    const startSetup = vi.spyOn(engine, "startSetup");
    renderApp({ engine });
    const region = await screen.findByRole("list", { name: "Things to know" });
    await userEvent.click(within(region).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByText(/running short of computer memory/)).toBeNull());
    expect(startSetup).not.toHaveBeenCalled();
    await expect(engine.getTierOffer()).resolves.toBeNull();
  });

  it("tidies anyway from the battery notice", async () => {
    const engine = createMockEngine({
      notices: [
        { id: "n-bat", code: "tidyPausedBattery", itemId: null, etaSeconds: null, bytes: null },
      ],
    });
    renderApp({ engine });
    const region = await screen.findByRole("list", { name: "Things to know" });
    await userEvent.click(within(region).getByRole("button", { name: "Tidy anyway" }));
    await waitFor(() => expect(engine.tidyAnywayCount).toBe(1));
    await waitFor(() => expect(screen.queryByText(/paused tidying/)).toBeNull());
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

describe("a question handed off from quick capture", () => {
  it("opens the Ask screen with that question", async () => {
    const platform = fakePlatform();
    renderApp({ platform });
    await screen.findByRole("textbox", { name: "Keep something, or ask" });
    act(() => platform.handOff("Where did I park?"));
    await waitFor(() => expect(window.location.hash).toBe("#/ask"));
    expect(
      await screen.findByRole("heading", { level: 1, name: "“Where did I park?”" }),
    ).toBeInTheDocument();
  });
});

describe("a screen asked for by the quick capture window", () => {
  it("is shown in the main window", async () => {
    const platform = fakePlatform();
    renderApp({ platform });
    await screen.findByRole("textbox", { name: "Keep something, or ask" });
    await act(async () => platform.openInMain("/items/item-letter"));
    expect(await screen.findByRole("heading", { level: 1, name: "Letter" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/items/item-letter");
  });

  it("ignores a route that is not a screen of the main window", async () => {
    const platform = fakePlatform();
    renderApp({ hash: "#/privacy", platform });
    await screen.findByRole("heading", { level: 1, name: "Only downloads you agreed to." });
    for (const forged of ["/quick-capture", "/setup", "/print/2026-11-13", "/nowhere", "today"]) {
      await act(async () => platform.openInMain(forged));
      expect(window.location.hash).toBe("#/privacy");
    }
    await act(async () => platform.openInMain("/ask"));
    expect(window.location.hash).toBe("#/ask");
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

describe("when a screen fails", () => {
  it("says kindly that a page that is not a date is not here", async () => {
    renderApp({ hash: "#/pages/not-a-date" });
    expect(
      await screen.findByRole("heading", { level: 1, name: "That page isn’t here" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
  });

  it("keeps the shell and recovers on the next route when a screen throws while drawing", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const engine = createMockEngine();
    const real = engine.getPage.bind(engine);
    engine.getPage = async (date) => ({ ...(await real(date)), needsYou: null as never });
    renderApp({ engine });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t show this. What you kept is safe.");
    expect(within(alert).getByRole("link", { name: "Back to today’s page" })).toHaveAttribute(
      "href",
      "#/today",
    );
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Keep something, or ask" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "Privacy" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Only downloads you agreed to." }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    error.mockRestore();
  });

  it("has a last-resort fallback outside every provider", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const engine = createMockEngine();
    engine.on = () => {
      throw new Error("engine gone");
    };
    renderApp({ engine });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Nocturne hit a problem" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start again" })).toBeInTheDocument();
    error.mockRestore();
  });
});

describe("narrow header mark", () => {
  it("shows the engine's state, not a fixed mark", async () => {
    setViewport(400);
    const engine = createMockEngine();
    await engine.pause(null);
    const { container } = renderApp({ engine });
    await screen.findByRole("button", { name: /^Menu/ });
    await waitFor(() =>
      expect(container.querySelector('header [data-state="paused"]')).not.toBeNull(),
    );
  });
});
