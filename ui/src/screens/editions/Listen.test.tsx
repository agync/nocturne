import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineError } from "../../engine/client";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

const paused = new WeakMap<HTMLMediaElement, boolean>();
const position = new WeakMap<HTMLMediaElement, number>();

beforeEach(() => {
  setViewport(1280);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(function (
    this: HTMLMediaElement,
  ) {
    paused.set(this, false);
    this.dispatchEvent(new Event("play"));
    return Promise.resolve();
  });
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(function (
    this: HTMLMediaElement,
  ) {
    paused.set(this, true);
    this.dispatchEvent(new Event("pause"));
  });
  Object.defineProperty(HTMLMediaElement.prototype, "currentTime", {
    configurable: true,
    get(this: HTMLMediaElement) {
      return position.get(this) ?? 0;
    },
    set(this: HTMLMediaElement, value: number) {
      position.set(this, value);
    },
  });
  Object.defineProperty(HTMLMediaElement.prototype, "paused", {
    configurable: true,
    get(this: HTMLMediaElement) {
      return paused.get(this) ?? true;
    },
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  window.location.hash = "";
});

const open = async (engine: MockEngine = createMockEngine()) => {
  renderApp({ hash: "#/listen/2026-11-13", engine });
  await screen.findByRole("list", { name: "Sections" });
  const audio = document.querySelector("audio") as HTMLAudioElement;
  return { engine, audio };
};

const at = (audio: HTMLAudioElement, seconds: number) => {
  act(() => {
    audio.currentTime = seconds;
    audio.dispatchEvent(new Event("timeupdate"));
  });
};

const status = () =>
  screen.getAllByRole("status").find((s) => s.classList.contains("sr-only")) as HTMLElement;

describe("Listen", () => {
  it("shows the page's headline, the sections in order and the player", async () => {
    const { audio } = await open();
    expect(screen.getByText(/^Listen · Friday 13 November · Voice: Emma$/)).toBeInTheDocument();
    const items = within(screen.getByRole("list", { name: "Sections" })).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("The night in one line");
    expect(items[0]).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Position" })).toBeInTheDocument();
    expect(audio.getAttribute("src")).toMatch(/^(blob:|mock-audio)/);
  });

  it("waits while the voice is made", async () => {
    const engine = createMockEngine();
    let release: () => void = () => undefined;
    const real = engine.speakPage.bind(engine);
    vi.spyOn(engine, "speakPage").mockImplementation(
      (date) => new Promise((resolve) => (release = () => resolve(real(date)))),
    );
    renderApp({ hash: "#/listen/2026-11-13", engine });
    expect(await screen.findByText(/Getting the voice ready/)).toBeInTheDocument();
    release();
    await screen.findByRole("list", { name: "Sections" });
  });

  it("keys_drive_the_player", async () => {
    const { audio } = await open();
    const play = vi.mocked(HTMLMediaElement.prototype.play);
    const pause = vi.mocked(HTMLMediaElement.prototype.pause);
    await userEvent.keyboard(" ");
    expect(play).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
    await userEvent.keyboard(" ");
    expect(pause).toHaveBeenCalledOnce();

    at(audio, 5);
    await userEvent.keyboard("{ArrowRight}");
    expect(audio.currentTime).toBe(15);
    await userEvent.keyboard("{ArrowLeft}");
    expect(audio.currentTime).toBe(5);
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(audio.currentTime).toBe(0);

    const second = screen.getAllByRole("listitem")[1] as HTMLElement;
    await userEvent.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(second).toHaveAttribute("aria-current", "true");
    expect(audio.currentTime).toBe(4);
    await userEvent.keyboard("{Shift>}{ArrowLeft}{/Shift}");
    expect(audio.currentTime).toBe(0);
  });

  it("previous restarts a section that has been playing, then goes back", async () => {
    const { audio } = await open();
    at(audio, 7.5);
    await userEvent.click(screen.getByRole("button", { name: "Previous section" }));
    expect(audio.currentTime).toBe(4);
    await userEvent.click(screen.getByRole("button", { name: "Previous section" }));
    expect(audio.currentTime).toBe(0);
  });

  it("a section can be chosen from the list", async () => {
    const { audio } = await open();
    const rows = screen.getAllByRole("listitem");
    await userEvent.click(within(rows[2] as HTMLElement).getByRole("button"));
    expect(audio.currentTime).toBe(8);
  });

  it("section_change_announced_once", async () => {
    const { audio } = await open();
    await userEvent.keyboard(" ");
    expect(status()).toHaveTextContent("Now reading: The night in one line");
    const text = status().textContent;
    at(audio, 1);
    at(audio, 2);
    at(audio, 3.5);
    expect(status().textContent).toBe(text);
    at(audio, 4.5);
    expect(status()).toHaveTextContent("Now reading: Needs you");
    const next = status().textContent;
    at(audio, 5);
    at(audio, 6);
    expect(status().textContent).toBe(next);
  });

  it("speed_saves_voice_speed", async () => {
    const { engine } = await open();
    const save = vi.spyOn(engine, "updateSettings");
    expect(screen.getByLabelText("Speed")).toHaveTextContent("1×");
    await userEvent.keyboard("]");
    expect(screen.getByLabelText("Speed")).toHaveTextContent("1.1×");
    await waitFor(() => expect(save).toHaveBeenCalledOnce());
    expect(save.mock.calls[0]?.[0]).toMatchObject({ voiceSpeed: 1.1, voice: null });
    expect((await engine.getSettings()).voiceSpeed).toBe(1.1);
    await userEvent.keyboard("[[[[[[");
    expect(screen.getByLabelText("Speed")).toHaveTextContent("0.8×");
    await userEvent.keyboard("[[");
    expect(save).toHaveBeenCalledTimes(4);
    await userEvent.click(screen.getByRole("button", { name: "Faster" }));
    expect(screen.getByLabelText("Speed")).toHaveTextContent("0.9×");
  });

  it("starts at the saved speed", async () => {
    const engine = createMockEngine();
    await engine.updateSettings({
      ...(await engine.getSettings()),
      voiceSpeed: 1.25,
      pageTime: null,
    } as never);
    const { audio } = await open(engine);
    expect(screen.getByLabelText("Speed")).toHaveTextContent("1.25×");
    expect(audio.playbackRate).toBe(1.25);
  });

  it("does not take keys while the person is choosing something else", async () => {
    const { audio } = await open();
    const play = vi.mocked(HTMLMediaElement.prototype.play);
    const faster = screen.getByRole("button", { name: "Faster" });
    faster.focus();
    fireEvent.keyDown(faster, { key: " " });
    expect(play).not.toHaveBeenCalled();
    expect(audio.currentTime).toBe(0);
  });

  it("skipped_line_shown", async () => {
    const engine = createMockEngine();
    const page = await engine.getPage("2026-11-13");
    const line = page.needsYou.find((l) => l.memoryIds.length > 0);
    await engine.setPrivate(line?.memoryIds[0] as string, true);
    await open(engine);
    expect(
      screen.getByText(/^Private items are never read aloud\. One was skipped\.$/),
    ).toBeVisible();
  });

  it("says nothing about private items when none were skipped", async () => {
    await open();
    expect(screen.queryByText(/Private items are never read aloud/)).toBeNull();
  });

  it("unavailable_state", async () => {
    const engine = createMockEngine();
    const speak = vi
      .spyOn(engine, "speakPage")
      .mockRejectedValueOnce(new EngineError("unavailable", "the voice is not ready"));
    renderApp({ hash: "#/listen/2026-11-13", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne is still getting ready, so it can’t read aloud yet.",
    );
    expect(screen.getByRole("link", { name: "Read the page" })).toHaveAttribute(
      "href",
      "#/pages/2026-11-13",
    );
    speak.mockRestore();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("list", { name: "Sections" });
  });

  it("says a missing page and a changing page in words", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "speakPage").mockRejectedValueOnce(new EngineError("notFound", "page"));
    renderApp({ hash: "#/listen/2026-11-13", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("There is no page for that day.");
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    cleanup();
    vi.spyOn(engine, "speakPage").mockRejectedValueOnce(new EngineError("busy", "changed"));
    renderApp({ hash: "#/listen/2026-11-13", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("The page changed while");
  });
});
