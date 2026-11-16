import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineError } from "../../engine/client";
import { createMockEngine } from "../../engine/mock/engine";
import { installFakeAudio } from "../../test/fakeAudio";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { comboFromEvent } from "./ShortcutField";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const open = async (engine = createMockEngine(), platform = fakePlatform()) => {
  renderApp({ hash: "#/settings", engine, platform });
  await screen.findByRole("heading", { level: 1, name: "Settings" });
  await screen.findByRole("combobox", { name: "Voice, name" });
  return { engine, platform };
};

describe("Settings", () => {
  it("shows every row as designed, with the current values", async () => {
    await open();
    expect(screen.getByRole("combobox", { name: "Morning page, time" })).toHaveDisplayValue(
      "07:00",
    );
    expect(
      screen.getByRole("combobox", { name: "Morning page, how it arrives" }),
    ).toHaveDisplayValue("Tell me when it’s ready");
    expect(screen.getByRole("combobox", { name: "Voice, name" })).toHaveDisplayValue(
      "Emma (British)",
    );
    expect(screen.getByRole("combobox", { name: "Voice, speed" })).toHaveDisplayValue("1×");
    expect(screen.getByText("Ctrl+Shift+Space")).toBeInTheDocument();
    expect(screen.getByText("Not set")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Start with this computer" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("combobox", { name: "Letting go" })).toHaveDisplayValue(
      "Normal (default) · fades after 30 days",
    );
    expect(screen.getByRole("combobox", { name: "Theme" })).toHaveDisplayValue("System");
    expect(screen.getByText("Language: English (UK)")).toBeInTheDocument();
  });

  it("reads each control with the hint of its row", async () => {
    await open();
    expect(
      screen.getByRole("combobox", { name: "Morning page, time" }),
    ).toHaveAccessibleDescription("When it is ready and how it arrives");
    expect(screen.getByRole("combobox", { name: "Voice, speed" })).toHaveAccessibleDescription(
      "For reading aloud",
    );
    expect(screen.getByRole("combobox", { name: "Letting go" })).toHaveAccessibleDescription(
      "How soon unused things fade",
    );
    expect(screen.getByRole("combobox", { name: "Theme" })).toHaveAccessibleDescription(
      "Follows this computer",
    );
    expect(
      screen.getByRole("button", { name: "Change quick keep shortcut" }),
    ).toHaveAccessibleDescription("Works from any app");
    expect(
      screen.getByRole("button", { name: "Choose inbox folder…" }),
    ).toHaveAccessibleDescription("Anything saved here is kept");
  });

  it("says how many days each letting-go pace waits", async () => {
    await open();
    const pace = screen.getByRole("combobox", { name: "Letting go" });
    expect(
      within(pace)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual([
      "Gentle · fades after 45 days",
      "Normal (default) · fades after 30 days",
      "Brisk · fades after 14 days",
    ]);
  });

  it("links the shortcut's instruction and its error to the box that takes the keys", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Change quick keep shortcut" }));
    const box = screen.getByRole("textbox", { name: "Quick keep shortcut, press the new keys" });
    expect(box).toHaveAccessibleDescription(/Works from any app.*Tab or Esc to cancel/);
    await userEvent.keyboard("k");
    expect(box).toHaveAttribute("aria-invalid", "true");
    expect(box).toHaveAccessibleDescription(/Include Ctrl, Alt or Super with another key\./);
  });

  it("spells out the engine's default shortcut", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      shortcut: "CommandOrControl+Shift+Space",
    });
    await open(engine);
    expect(screen.getByText("Ctrl+Shift+Space")).toBeInTheDocument();
  });

  it("links the explanation of a shortcut that is not working to its button", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({ ...base, shortcutAvailable: false });
    await open(engine);
    expect(
      screen.getByRole("button", { name: "Change quick keep shortcut" }),
    ).toHaveAccessibleDescription(/This shortcut isn’t working here/);
  });

  it("offers no update checks, because nothing in this version can update itself", async () => {
    await open();
    expect(screen.queryByRole("switch", { name: /update/i })).toBeNull();
    expect(screen.queryByText(/update/i)).toBeNull();
  });

  it("settings.no_auto_print_or_reminders", async () => {
    await open();
    expect(screen.queryByRole("switch", { name: "Print the page" })).toBeNull();
    expect(screen.queryByRole("switch", { name: "Reminders" })).toBeNull();
    expect(screen.queryByText(/remind/i)).toBeNull();
  });

  it("saves a change at once and says Saved.", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "updateSettings");
    const before = screen.getByRole("switch", { name: "Start with this computer" });
    const was = before.getAttribute("aria-checked") === "true";
    await userEvent.click(before);
    expect(spy).toHaveBeenCalledWith(
      expect.objectContaining({ startAtLogin: !was, pageTime: null }),
    );
    expect(screen.getByRole("switch", { name: "Start with this computer" })).toHaveAttribute(
      "aria-checked",
      String(!was),
    );
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
  });

  it("says to move Nocturne to Applications when start with this computer is refused for that", async () => {
    const { engine } = await open();
    vi.spyOn(engine, "updateSettings").mockRejectedValueOnce(
      new EngineError("unavailable", "move-to-applications"),
    );
    const toggle = screen.getByRole("switch", { name: "Start with this computer" });
    const was = toggle.getAttribute("aria-checked");
    await userEvent.click(toggle);
    expect(
      await screen.findByText("Move Nocturne to the Applications folder first, then turn this on."),
    ).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Start with this computer" })).toHaveAttribute(
      "aria-checked",
      was,
    );
  });

  it("says nothing was saved for any other refusal", async () => {
    const { engine } = await open();
    vi.spyOn(engine, "updateSettings").mockRejectedValueOnce(
      new EngineError("unavailable", "start with this computer could not be changed"),
    );
    await userEvent.click(screen.getByRole("switch", { name: "Start with this computer" }));
    expect(await screen.findByText("That didn’t save. Nothing was changed.")).toBeInTheDocument();
  });

  it("changes the page time, delivery, voice, speed, pace and theme", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Morning page, time" }),
      "06:30",
    );
    expect(spy).toHaveBeenLastCalledWith(expect.objectContaining({ pageTime: "06:30" }));
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Morning page, how it arrives" }),
      "quiet",
    );
    expect(spy).toHaveBeenLastCalledWith(expect.objectContaining({ delivery: "quiet" }));
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Voice, name" }),
      "bm_george",
    );
    expect(spy).toHaveBeenLastCalledWith(expect.objectContaining({ voice: "bm_george" }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Voice, speed" }), "1.25");
    expect(spy).toHaveBeenLastCalledWith(expect.objectContaining({ voiceSpeed: 1.25 }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Letting go" }), "brisk");
    expect(spy).toHaveBeenLastCalledWith(expect.objectContaining({ lettingGo: "brisk" }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Theme" }), "dark");
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
  });

  it("puts a change back and says so when saving fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "updateSettings").mockRejectedValue(new Error("no"));
    await open(engine);
    const toggle = screen.getByRole("switch", { name: "Start with this computer" });
    const was = toggle.getAttribute("aria-checked");
    await userEvent.click(toggle);
    expect(await screen.findByText("That didn’t save. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Start with this computer" })).toHaveAttribute(
      "aria-checked",
      was,
    );
  });

  it("chooses the inbox folder with the system's folder picker", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockResolvedValue("/data/Nocturne inbox");
    const { engine } = await open(createMockEngine(), platform);
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.click(screen.getByRole("button", { name: "Choose inbox folder…" }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ inboxFolder: "/data/Nocturne inbox" }),
      ),
    );
    expect(await screen.findByText("/data/Nocturne inbox")).toBeInTheDocument();
  });

  it("captures a new shortcut, and Esc and Tab get out without changing it", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.click(screen.getByRole("button", { name: "Change quick keep shortcut" }));
    const field = screen.getByRole("textbox", { name: /press the new keys/ });
    expect(field).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: /press the new keys/ })).toBeNull();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Change quick keep shortcut" })).toHaveFocus(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Change quick keep shortcut" }));
    await userEvent.tab();
    expect(screen.queryByRole("textbox", { name: /press the new keys/ })).toBeNull();
    expect(spy).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Change quick keep shortcut" }));
    await userEvent.keyboard("a");
    expect(await screen.findByRole("alert")).toHaveTextContent("Include Ctrl, Alt or Super");
    await userEvent.keyboard("{Control>}{Alt>}k{/Alt}{/Control}");
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ shortcut: "Ctrl+Alt+K" }));
  });

  it("says when the shortcut is not working, and offers the command for a desktop shortcut", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({ ...base, shortcutAvailable: false });
    await open(engine);
    expect(screen.getByText(/This shortcut isn’t working here/)).toBeInTheDocument();
    expect(await screen.findByText("nocturne --quick-capture")).toBeInTheDocument();
  });

  it("shows the keys the desktop bound and opens the desktop's own settings to change them", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      desktopShortcut: { state: "held", keys: "Ctrl+Alt+K", configurable: true },
    });
    await open(engine);
    expect(screen.getByText("Ctrl+Alt+K")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change quick keep shortcut" })).toBeNull();
    const button = screen.getByRole("button", { name: "Change in desktop settings" });
    expect(button).toHaveAccessibleDescription(/Your desktop looks after this shortcut\./);
    await userEvent.click(button);
    expect(engine.shortcutSettingsOpened).toBe(1);
    expect(screen.queryByText(/isn’t working here/)).toBeNull();
  });

  it("says so when the desktop's shortcut settings do not open, and lets the person try again", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      desktopShortcut: { state: "held", keys: "Ctrl+Alt+K", configurable: true },
    });
    const configure = vi.spyOn(engine, "configureShortcut").mockRejectedValueOnce(new Error("no"));
    await open(engine);
    const button = screen.getByRole("button", { name: "Change in desktop settings" });
    await userEvent.click(button);
    expect(
      await screen.findByText(
        "Your desktop’s shortcut settings didn’t open. Change the keys in your desktop’s keyboard settings instead.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Your desktop looks after this shortcut.")).toBeNull();
    expect(button).toHaveAccessibleDescription(/shortcut settings didn’t open/);
    await userEvent.click(button);
    expect(configure).toHaveBeenCalledTimes(2);
    expect(await screen.findByText("Your desktop looks after this shortcut.")).toBeInTheDocument();
  });

  it("says where to change the shortcut when the desktop cannot open its settings", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      desktopShortcut: { state: "held", keys: null, configurable: false },
    });
    await open(engine);
    expect(screen.getByText(base.shortcut)).toBeInTheDocument();
    expect(
      screen.getByText(
        "Your desktop looks after this shortcut. Change it in your desktop’s keyboard settings.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change in desktop settings" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Change quick keep shortcut" })).toBeNull();
  });

  it("offers to ask the desktop again when the shortcut was not added", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      shortcutAvailable: true,
      desktopShortcut: { state: "declined", keys: null, configurable: true },
    });
    const retry = vi.spyOn(engine, "retryShortcut");
    await open(engine);
    expect(
      screen.getByText(/The shortcut wasn’t added\. Choose Try again, or add a desktop shortcut\./),
    ).toBeInTheDocument();
    expect(await screen.findByText("nocturne --quick-capture")).toBeInTheDocument();
    expect(screen.queryByText(/isn’t working here/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
  });

  it("says the desktop is asking, and offers nothing to press, while it decides", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({
      ...base,
      shortcut: "CommandOrControl+Shift+Space",
      shortcutAvailable: true,
      desktopShortcut: { state: "asking", keys: null, configurable: false },
    });
    await open(engine);
    expect(screen.getByText("Ctrl+Shift+Space")).toBeInTheDocument();
    expect(
      screen.getByText("Your desktop is asking whether to add this shortcut."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/isn’t working here/)).toBeNull();
    expect(screen.queryByText(/wasn’t added/)).toBeNull();
    expect(screen.queryByRole("button", { name: /Change/ })).toBeNull();
  });

  it("asks nothing about a desktop shortcut while the shortcut works", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "quickKeepCommand");
    await open(engine);
    expect(spy).not.toHaveBeenCalled();
  });

  it("settings.stop_watching_sends_empty", async () => {
    const engine = createMockEngine();
    await engine.updateSettings({
      ...(await engine.getSettings()),
      inboxFolder: "/data/Nocturne inbox",
      pageTime: null,
    } as never);
    await open(engine);
    const spy = vi.spyOn(engine, "updateSettings");
    expect(await screen.findByText("/data/Nocturne inbox")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop watching the inbox folder" }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({ inboxFolder: "" })),
    );
    expect(await screen.findByText("Not set")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop watching the inbox folder" })).toBeNull();
  });

  it("offers no stop button when nothing is watched", async () => {
    await open();
    expect(screen.queryByRole("button", { name: /Stop watching/ })).toBeNull();
  });

  it("links to advanced details", async () => {
    await open();
    expect(screen.getByRole("link", { name: "Show advanced details" })).toHaveAttribute(
      "href",
      "#/settings/advanced",
    );
  });

  it("says so and offers to try again when settings cannot be read", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "getSettings").mockRejectedValueOnce(new Error("no"));
    renderApp({ hash: "#/settings", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("Nocturne couldn’t open settings.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});

describe("comboFromEvent", () => {
  const e = (
    key: string,
    mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {},
  ) => ({
    key,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  it("names combinations and asks for a modifier", () => {
    expect(comboFromEvent(e(" ", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+Space");
    expect(comboFromEvent(e("k", { metaKey: true }))).toBe("Super+K");
    expect(comboFromEvent(e("F5", { altKey: true }))).toBe("Alt+F5");
    expect(comboFromEvent(e("a"))).toBe("modifier");
    expect(comboFromEvent(e("Shift", { shiftKey: true }))).toBeNull();
  });
});

describe("Advanced details", () => {
  const openAdvanced = async (platform = fakePlatform()) => {
    const engine = createMockEngine();
    renderApp({ hash: "#/settings/advanced", engine, platform });
    await screen.findByRole("heading", { level: 1, name: "Advanced details" });
    await screen.findByText("tier");
    return { engine, platform };
  };

  it("counts one memory or one item in the singular", async () => {
    const engine = createMockEngine();
    const real = engine.getAdvanced.bind(engine);
    vi.spyOn(engine, "getAdvanced").mockImplementation(async () => {
      const view = await real();
      return {
        ...view,
        memoryCount: 1,
        itemCount: 1,
        lastTidy: view.lastTidy && { ...view.lastTidy, items: 1 },
      };
    });
    renderApp({ hash: "#/settings/advanced", engine });
    expect(
      await screen.findByText(/schema .* · 1 memory · 1 item · integrity/),
    ).toBeInTheDocument();
    expect(screen.getByText(/ · 1 item · \d+ merged/)).toBeInTheDocument();
  });

  it("shows the technical details, and only here", async () => {
    await openAdvanced();
    expect(screen.getByText("For the curious. Nothing here needs changing.")).toBeInTheDocument();
    expect(
      screen.getByText("standard · 32 GB RAM · Radeon 780M (iGPU) · no NPU"),
    ).toBeInTheDocument();
    expect(screen.getByText("Qwen3.5-9B-GGUF · 5.9 GB · vulkan")).toBeInTheDocument();
    expect(
      screen.getByText(/Lemonade 8\.2\.2 · 127\.0\.0\.1:8020 · up 3 d 4 h · 0 restarts/),
    ).toBeInTheDocument();
    expect(screen.getByText(/integrity ok/)).toBeInTheDocument();
    expect(screen.getByText("02:10–02:16 · 14 items · 3 merged · 3 let go")).toBeInTheDocument();
    expect(screen.getByText(/voice “bf_emma”/)).toBeInTheDocument();
  });

  it("says whether this window can record, and why not", async () => {
    await openAdvanced();
    expect(screen.getByText("recording")).toBeInTheDocument();
    expect(screen.getByText("unavailable (no media devices)")).toBeInTheDocument();
  });

  it("says recording is available when it is", async () => {
    installFakeAudio();
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: { getUserMedia: vi.fn() },
    });
    await openAdvanced();
    expect(screen.getByText("available")).toBeInTheDocument();
    vi.unstubAllGlobals();
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: undefined });
  });

  it("opens the logs, and goes back into setup", async () => {
    const platform = fakePlatform();
    const spy = vi.spyOn(platform, "openLogs");
    await openAdvanced(platform);
    await userEvent.click(screen.getByRole("button", { name: "Open logs" }));
    expect(spy).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "Choose a different mind…" }));
    expect(window.location.hash).toBe("#/setup?step=choosing");
  });

  it("says so when the logs cannot be opened", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "openLogs").mockRejectedValue(new Error("there are no logs yet"));
    await openAdvanced(platform);
    await userEvent.click(screen.getByRole("button", { name: "Open logs" }));
    expect(
      await screen.findByText("That didn’t work. Nothing was lost; try again in a moment."),
    ).toBeInTheDocument();
  });

  it("resets with the same typed phrase as delete everything", async () => {
    const { engine } = await openAdvanced();
    const spy = vi.spyOn(engine, "deleteEverything");
    await userEvent.click(screen.getByRole("button", { name: "Reset Nocturne…" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset Nocturne?" });
    await userEvent.type(within(dialog).getByRole("textbox"), "delete everything");
    await userEvent.click(within(dialog).getByRole("button", { name: "Reset Nocturne" }));
    expect(spy).toHaveBeenCalledWith("delete everything");
    await waitFor(() => expect(window.location.hash).toBe("#/setup?step=welcome"));
  });
});

describe("settings", () => {
  it("settings.away_row_value: shows Off, and then how it is on with how many phones", async () => {
    await open();
    const row = (await screen.findByRole("link", {
      name: "Open Away from home, Off",
    })) as HTMLAnchorElement;
    expect(row).toHaveAttribute("href", "#/settings/away");
    expect(row).toHaveAccessibleDescription("Use Nocturne from your phone");
    document.body.innerHTML = "";
    await open(createMockEngine({ remote: { auto: false, scenario: "on" } }));
    expect(
      await screen.findByRole("link", { name: "Open Away from home, On · Simple · 2 phones" }),
    ).toBeInTheDocument();
    document.body.innerHTML = "";
    await open(createMockEngine({ remote: { auto: false, scenario: "relink" } }));
    expect(
      await screen.findByRole("link", { name: "Open Away from home, Phones need the new address" }),
    ).toBeInTheDocument();
  });
});
