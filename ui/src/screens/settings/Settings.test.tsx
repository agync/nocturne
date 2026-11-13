import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
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
    expect(screen.getByRole("switch", { name: "Print the page" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
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
      "Gentle (default)",
    );
    expect(screen.getByRole("combobox", { name: "Theme · language, theme" })).toHaveDisplayValue(
      "System",
    );
    expect(screen.getByText("English (UK)")).toBeInTheDocument();
  });

  it("has update checks off by default, and reminders opt-in", async () => {
    await open();
    expect(screen.getByRole("switch", { name: "Checking for updates" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(screen.getByText("Off. Nocturne only checks when you ask.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Reminders" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("saves a change at once and says Saved.", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.click(screen.getByRole("switch", { name: "Print the page" }));
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ autoPrint: true, pageTime: null }));
    expect(screen.getByRole("switch", { name: "Print the page" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
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
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Theme · language, theme" }),
      "dark",
    );
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
  });

  it("puts a change back and says so when saving fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "updateSettings").mockRejectedValue(new Error("no"));
    await open(engine);
    await userEvent.click(screen.getByRole("switch", { name: "Reminders" }));
    expect(await screen.findByText("That didn’t save. Nothing was changed.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Reminders" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("chooses the inbox folder with the system's folder picker", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockResolvedValue("/data/Nocturne inbox");
    const { engine } = await open(createMockEngine(), platform);
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
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

  it("says when the shortcut is taken by something else", async () => {
    const engine = createMockEngine();
    const base = await engine.getSettings();
    vi.spyOn(engine, "getSettings").mockResolvedValue({ ...base, shortcutAvailable: false });
    await open(engine);
    expect(screen.getByText(/already uses this shortcut/)).toBeInTheDocument();
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
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t open settings.");
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

  it("opens the logs, and goes back into setup", async () => {
    const platform = fakePlatform();
    const spy = vi.spyOn(platform, "openPath");
    await openAdvanced(platform);
    await userEvent.click(screen.getByRole("button", { name: "Open logs" }));
    expect(spy).toHaveBeenCalledWith("~/.local/share/nocturne/logs");
    await userEvent.click(screen.getByRole("button", { name: "Choose a different mind…" }));
    expect(window.location.hash).toBe("#/setup?step=choosing");
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
