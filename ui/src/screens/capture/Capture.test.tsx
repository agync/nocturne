import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { type FakePlatform, fakePlatform, renderApp, setViewport } from "../../test/render";
import { CLOSE_AFTER_MS, MIN_HOLD_MS } from "./index";

class FakeRecorder {
  state: "inactive" | "recording" = "inactive";
  mimeType = "audio/webm";
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob([new Uint8Array([1, 2, 3, 4])]) });
    this.onstop?.();
  }
}

const track = { stop: vi.fn() };
let getUserMedia: ReturnType<typeof vi.fn>;

beforeEach(() => {
  setViewport(560);
  vi.stubGlobal("MediaRecorder", FakeRecorder);
  getUserMedia = vi.fn().mockResolvedValue({ getTracks: () => [track] });
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { getUserMedia },
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  window.location.hash = "";
});

interface Opened {
  engine: MockEngine;
  platform: FakePlatform;
  close: ReturnType<typeof vi.fn>;
  handOff: ReturnType<typeof vi.fn>;
}

const open = async (): Promise<Opened> => {
  const platform = fakePlatform();
  const close = vi.fn(async () => undefined);
  const handOff = vi.fn(async () => undefined);
  platform.closeQuickCapture = close;
  platform.handOffAsk = handOff;
  const engine = createMockEngine();
  renderApp({ hash: "#/quick-capture", engine, platform });
  await screen.findByRole("textbox", { name: "Keep something, or ask" });
  return { engine, platform, close, handOff };
};

/** The window's own announcer; the page-wide toast region is a status too. */
const live = () =>
  screen
    .getAllByRole("status")
    .find((s) => s.getAttribute("aria-atomic") === "true") as HTMLElement;

const box = () => screen.getByRole("textbox", { name: "Keep something, or ask" });

describe("quick capture, empty", () => {
  it("opens focused with the two hints and no shell around it", async () => {
    await open();
    expect(box()).toHaveFocus();
    expect(screen.getByText("Esc closes")).toBeInTheDocument();
    expect(screen.getByText(/Type, paste or drop · hold/)).toHaveTextContent(
      "hold Space on empty to talk",
    );
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Keep" })).toBeNull();
  });

  it("Escape closes the window", async () => {
    const { close } = await open();
    await userEvent.keyboard("{Escape}");
    expect(close).toHaveBeenCalledOnce();
  });
});

describe("keeping words", () => {
  it("Enter keeps a note, says Kept, announces it and closes after 1.6 seconds", async () => {
    const { engine, close } = await open();
    const spy = vi.spyOn(engine, "keepText");
    await userEvent.type(box(), "Buy milk{Enter}");
    expect(spy).toHaveBeenCalledWith("Buy milk", "quickCapture");
    expect(await screen.findByText("Kept.", { selector: "strong" })).toBeInTheDocument();
    expect(screen.getByText("I’ll read it tonight.")).toBeInTheDocument();
    expect(live()).toHaveTextContent("Kept. I’ll read it tonight.");
    expect(close).not.toHaveBeenCalled();
    await waitFor(() => expect(close).toHaveBeenCalledOnce(), { timeout: CLOSE_AFTER_MS + 1500 });
  });

  it("the two buttons appear with text and the default verb is primary", async () => {
    await open();
    await userEvent.type(box(), "Is Leo still swimming");
    expect(screen.getByRole("button", { name: /^Ask/ })).toHaveAttribute("data-primary", "true");
    expect(screen.getByRole("button", { name: /^Keep/ })).toHaveAttribute("data-primary", "false");
    await userEvent.clear(box());
    await userEvent.type(box(), "Buy milk");
    expect(screen.getByRole("button", { name: /^Keep/ })).toHaveAttribute("data-primary", "true");
  });

  it("Ctrl Enter always keeps, even a question", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepText");
    await userEvent.type(box(), "When is the dentist?{Control>}{Enter}{/Control}");
    expect(spy).toHaveBeenCalledWith("When is the dentist?", "quickCapture");
  });

  it("Undo takes it back, stays while shown and the window does not close", async () => {
    const { engine, close } = await open();
    const spy = vi.spyOn(engine, "unkeep");
    await userEvent.type(box(), "Buy milk{Enter}");
    const undo = await screen.findByRole("button", { name: "Undo" });
    await userEvent.click(undo);
    expect(spy).toHaveBeenCalledOnce();
    expect(live()).toHaveTextContent("Taken back. Nothing was kept.");
    expect(box()).toBeInTheDocument();
    expect(close).not.toHaveBeenCalled();
  });

  it("does not close while the pill is hovered or has focus", async () => {
    const { close } = await open();
    await userEvent.type(box(), "Buy milk{Enter}");
    const undo = await screen.findByRole("button", { name: "Undo" });
    vi.useFakeTimers();
    act(() => undo.focus());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CLOSE_AFTER_MS * 3);
    });
    expect(close).not.toHaveBeenCalled();
    act(() => undo.blur());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CLOSE_AFTER_MS + 50);
    });
    expect(close).toHaveBeenCalledOnce();
  });

  it("says a duplicate kindly and offers to open it", async () => {
    const { close, platform } = await open();
    await userEvent.type(box(), "Gran’s new number ends 4471.{Enter}");
    expect(await screen.findByText(/You already kept this note on/)).toHaveTextContent(
      "Nothing new to add.",
    );
    expect(screen.queryByRole("button", { name: "Keep anyway" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open it" }));
    expect(platform.opened()).toBe("/items/item-gran");
    expect(close).toHaveBeenCalledOnce();
  });

  it("keeps the text and says so when keeping fails", async () => {
    const { engine } = await open();
    vi.spyOn(engine, "keepText").mockRejectedValue(new Error("no"));
    await userEvent.type(box(), "Buy milk{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t keep that just now");
    expect(box()).toHaveValue("Buy milk");
  });
});

describe("asking", () => {
  it("a question goes to the main window and the window closes", async () => {
    const { handOff, close } = await open();
    const spy = vi.spyOn(askRequests, "request");
    await userEvent.type(box(), "When is the dentist?{Enter}");
    expect(handOff).toHaveBeenCalledWith("When is the dentist?");
    expect(spy).toHaveBeenCalledWith("When is the dentist?");
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
  });

  it("Alt Enter asks even when it is not a question", async () => {
    const { handOff } = await open();
    await userEvent.type(box(), "Swimming{Alt>}{Enter}{/Alt}");
    expect(handOff).toHaveBeenCalledWith("Swimming");
  });
});

describe("pasting", () => {
  it("web content shows the dashed unconfirmed chip and is kept as a web page", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepText");
    fireEvent.paste(box(), {
      clipboardData: {
        files: [],
        types: ["text/plain", "text/html"],
        getData: () => "Leo is allergic to nuts so I always…",
      },
    });
    await userEvent.type(box(), "Nut-free lunchbox ideas");
    expect(screen.getByText("From a web page · kept as unconfirmed")).toBeInTheDocument();
    expect(screen.getByText("Web pages are never treated as true until you say so.")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: /^Keep/ }));
    expect(spy).toHaveBeenCalledWith("Nut-free lunchbox ideas", "pastedWeb");
    expect(await screen.findByText("Kept as unconfirmed. I’ll read it tonight.")).toBeVisible();
  });

  it("plain text has no chip", async () => {
    await open();
    fireEvent.paste(box(), { clipboardData: { files: [], types: ["text/plain"] } });
    await userEvent.type(box(), "Milk");
    expect(screen.queryByText("From a web page · kept as unconfirmed")).toBeNull();
  });

  it("an image is kept as bytes at once", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepBytes");
    const file = new File([new Uint8Array([9, 9, 9])], "x.png", { type: "image/png" });
    fireEvent.paste(box(), { clipboardData: { files: [file], types: ["Files"] } });
    expect(await screen.findByText("Photo. I’ll read it tonight.")).toBeVisible();
    expect(spy).toHaveBeenCalledOnce();
    expect(spy.mock.calls[0]?.[1]).toBe("image/png");
  });
});

describe("dropping files", () => {
  it("keeps them as dropped", async () => {
    const { engine, platform } = await open();
    const spy = vi.spyOn(engine, "keepFiles");
    act(() => platform.drop(["/tmp/a.jpg"]));
    expect(spy).toHaveBeenCalledWith(["/tmp/a.jpg"], "drop");
    expect(await screen.findByText("Kept.", { selector: "strong" })).toBeInTheDocument();
  });
});

describe("recording", () => {
  /** Holds Space for `ms` of the clock, then leaves the key down. */
  const hold = async (ms = 0) => {
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    await screen.findByText("Listening");
    if (ms > 0) now.mockReturnValue(start + ms);
  };
  const start = 1_700_000_000_000;
  let now: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    now = vi.spyOn(Date, "now").mockReturnValue(start);
  });

  it("hold Space on empty records with a meter and timer, release keeps a voice note", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    await hold(1200);
    expect(live()).toHaveTextContent("Recording started");
    expect(screen.getByText("Release Space to keep · long recordings are fine")).toBeVisible();
    expect(screen.getByRole("button", { name: "Discard" })).toBeVisible();
    expect(box()).toHaveValue("");
    fireEvent.keyUp(box(), { key: " " });
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    expect(await screen.findByText(/Voice note, 0:0\d\. Reading it now\./)).toBeVisible();
    expect(live()).toHaveTextContent(/^Kept\. Voice note, .*second/);
    expect(track.stop).toHaveBeenCalled();
  });

  it("Space with text in the box is a space, not a recording", async () => {
    await open();
    await userEvent.type(box(), "a b");
    expect(box()).toHaveValue("a b");
    expect(getUserMedia).not.toHaveBeenCalled();
  });

  it("Discard keeps nothing and says so", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    await hold();
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(live()).toHaveTextContent("Discarded. Nothing was kept.");
    expect(spy).not.toHaveBeenCalled();
    expect(screen.queryByText("Listening")).toBeNull();
  });

  it("Escape while recording discards it, a second Escape closes", async () => {
    const { close } = await open();
    await hold();
    fireEvent.keyDown(box(), { key: "Escape" });
    await waitFor(() => expect(screen.queryByText("Listening")).toBeNull());
    expect(close).not.toHaveBeenCalled();
    fireEvent.keyDown(box(), { key: "Escape" });
    expect(close).toHaveBeenCalledOnce();
  });

  it("the microphone button starts and the Keep button ends it", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    await userEvent.click(screen.getByRole("button", { name: "Record a voice note" }));
    await screen.findByText("Listening");
    expect(
      screen.getByText("Press Keep when you are done · long recordings are fine"),
    ).toBeVisible();
    now.mockReturnValue(start + MIN_HOLD_MS + 100);
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
  });

  it("explains a turned-off microphone with the shared notice", async () => {
    await open();
    getUserMedia.mockRejectedValue(new DOMException("no", "NotAllowedError"));
    await userEvent.click(screen.getByRole("button", { name: "Record a voice note" }));
    expect(
      await screen.findByText("Nocturne can’t hear you. Microphone access is turned off for it."),
    ).toBeInTheDocument();
  });
});

describe("the turned-off microphone notice", () => {
  it("notice.mic_opens_sound_settings", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "openSoundSettings");
    getUserMedia.mockRejectedValue(new DOMException("no", "NotAllowedError"));
    await userEvent.click(screen.getByRole("button", { name: "Record a voice note" }));
    await userEvent.click(await screen.findByRole("button", { name: "Open sound settings" }));
    expect(spy).toHaveBeenCalledOnce();
  });
});

describe("a slow microphone", () => {
  const start = 1_700_000_000_000;
  let now: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    now = vi.spyOn(Date, "now").mockReturnValue(start);
  });

  /** getUserMedia that answers only when the test says so, like a permission prompt. */
  const late = () => {
    let release: () => void = () => undefined;
    getUserMedia.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () => resolve({ getTracks: () => [track] });
        }),
    );
    return () => release();
  };

  it("a tap of Space released before the microphone opens does not leave it recording", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    const open_ = late();
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    now.mockReturnValue(start + MIN_HOLD_MS - 100);
    fireEvent.keyUp(box(), { key: " " });
    await act(async () => open_());
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
    expect(screen.queryByText("Listening")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });

  it("a long press released before the microphone opens keeps no empty recording", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    const open_ = late();
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    now.mockReturnValue(start + MIN_HOLD_MS + 500);
    fireEvent.keyUp(box(), { key: " " });
    await act(async () => open_());
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
    expect(screen.queryByText("Listening")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });

  it("Escape while the microphone is still opening leaves nothing recording", async () => {
    const { close, engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    const open_ = late();
    track.stop.mockClear();
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    fireEvent.keyDown(box(), { key: "Escape" });
    expect(close).not.toHaveBeenCalled();
    await act(async () => open_());
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
    expect(screen.queryByText("Listening")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });

  it("closing the window while the microphone opens releases it when it does", async () => {
    await open();
    const open_ = late();
    track.stop.mockClear();
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    cleanup();
    await act(async () => open_());
    expect(track.stop).toHaveBeenCalled();
  });
});

describe("taking back", () => {
  it("Undo then a second keep still closes by itself", async () => {
    const { close } = await open();
    await userEvent.type(box(), "Buy milk{Enter}");
    const pill = await screen.findByText("Kept.", { selector: "strong" });
    // The pointer is on the pill when Undo is pressed, so the pill never sees it leave.
    await userEvent.hover(pill);
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    await userEvent.type(box(), "Buy eggs{Enter}");
    await screen.findByText("Kept.", { selector: "strong" });
    await waitFor(() => expect(close).toHaveBeenCalledOnce(), { timeout: CLOSE_AFTER_MS + 1500 });
  });

  it("Undo after several files takes back every new one and says so truthfully", async () => {
    const { engine, platform } = await open();
    const spy = vi.spyOn(engine, "unkeep");
    act(() => platform.drop(["/tmp/a.jpg", "/tmp/b.jpg", "/tmp/c.jpg"]));
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    expect(spy).toHaveBeenCalledTimes(3);
    expect(live()).toHaveTextContent("Taken back. Nothing was kept.");
    expect(await engine.listItems({ since: null, status: null, limit: null })).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ title: "a.jpg" })]),
    );
  });

  it("never takes back a duplicate: only the new file in a mixed drop", async () => {
    const { engine, platform } = await open();
    const spy = vi.spyOn(engine, "unkeep");
    act(() => platform.drop(["/tmp/council-tax-2026.pdf", "/tmp/fresh.jpg"]));
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy.mock.calls[0]?.[0]).not.toBe("item-counciltax");
    await expect(engine.getItem("item-counciltax")).resolves.toBeDefined();
  });

  it("a drop of only duplicates says so and offers no Undo", async () => {
    const { platform } = await open();
    act(() => platform.drop(["/tmp/council-tax-2026.pdf"]));
    expect(await screen.findByText(/You already kept this document on/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });

  it("Ctrl Z takes it back from the keyboard while the pill is shown", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "unkeep");
    await userEvent.type(box(), "Buy milk{Enter}");
    await screen.findByText("Kept.", { selector: "strong" });
    expect(screen.getByRole("main")).toHaveFocus();
    await userEvent.keyboard("{Control>}z{/Control}");
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    expect(live()).toHaveTextContent("Taken back. Nothing was kept.");
  });
});

describe("leaving from every view", () => {
  it("Escape closes from the Kept. pill", async () => {
    const { close } = await open();
    await userEvent.type(box(), "Buy milk{Enter}");
    await screen.findByText("Kept.", { selector: "strong" });
    await userEvent.keyboard("{Escape}");
    expect(close).toHaveBeenCalledOnce();
  });

  it("Escape and a Close button leave the duplicate view", async () => {
    const { close } = await open();
    await userEvent.type(box(), "Gran’s new number ends 4471.{Enter}");
    await screen.findByText(/You already kept this note/);
    await userEvent.keyboard("{Escape}");
    expect(close).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(close).toHaveBeenCalledTimes(2);
  });
});

describe("keeping once", () => {
  it("holding Enter keeps one thing, not many", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepText");
    await userEvent.type(box(), "Buy milk");
    fireEvent.keyDown(box(), { key: "Enter" });
    fireEvent.keyDown(box(), { key: "Enter", repeat: true });
    fireEvent.keyDown(box(), { key: "Enter", repeat: true });
    await screen.findByText("Kept.", { selector: "strong" });
    expect(spy).toHaveBeenCalledOnce();
  });

  it("a second Enter while the first keep is still on its way does nothing", async () => {
    const { engine } = await open();
    let finish: () => void = () => undefined;
    const real = engine.keepText.bind(engine);
    const spy = vi.spyOn(engine, "keepText").mockImplementation(
      (text, source) =>
        new Promise((resolve) => {
          finish = () => resolve(real(text, source));
        }),
    );
    await userEvent.type(box(), "Buy milk");
    fireEvent.keyDown(box(), { key: "Enter" });
    fireEvent.keyDown(box(), { key: "Enter" });
    expect(spy).toHaveBeenCalledOnce();
    await act(async () => finish());
    await screen.findByText("Kept.", { selector: "strong" });
  });
});

describe("when things go wrong", () => {
  it("a failed hand-off says so and the window still answers", async () => {
    const { handOff, close } = await open();
    handOff.mockRejectedValueOnce(new Error("no"));
    await userEvent.type(box(), "When is the dentist?{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t keep that just now");
    expect(close).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: /^Ask/ }));
    await waitFor(() => expect(handOff).toHaveBeenCalledTimes(2));
  });

  it("a second Ctrl Z while the first is working does nothing", async () => {
    const { engine, platform } = await open();
    const real = engine.unkeep.bind(engine);
    let finish: () => void = () => undefined;
    const spy = vi.spyOn(engine, "unkeep").mockImplementation(
      (id) =>
        new Promise((resolve) => {
          finish = () => resolve(real(id));
        }),
    );
    act(() => platform.drop(["/tmp/a.jpg", "/tmp/b.jpg"]));
    await screen.findByRole("button", { name: "Undo" });
    fireEvent.keyDown(screen.getByRole("main"), { key: "z", ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("main"), { key: "z", ctrlKey: true });
    expect(spy).toHaveBeenCalledTimes(2);
    await act(async () => finish());
  });

  it("when one take-back fails it says what stayed", async () => {
    const { engine, platform } = await open();
    const real = engine.unkeep.bind(engine);
    let calls = 0;
    vi.spyOn(engine, "unkeep").mockImplementation(async (id) => {
      calls += 1;
      if (calls === 2) throw new Error("no");
      return real(id);
    });
    act(() => platform.drop(["/tmp/a.jpg", "/tmp/b.jpg"]));
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() =>
      expect(live()).toHaveTextContent("Taken back 1 of 2. The other one is still kept."),
    );
    expect(live()).not.toHaveTextContent("Nothing was kept");
    expect(screen.getByRole("button", { name: "Undo" })).toBeVisible();
  });

  it("a second paste while the first is being kept does nothing", async () => {
    const { engine } = await open();
    let finish: () => void = () => undefined;
    const real = engine.keepBytes.bind(engine);
    const spy = vi.spyOn(engine, "keepBytes").mockImplementation(
      (bytes, mime, source) =>
        new Promise((resolve) => {
          finish = () => resolve(real(bytes, mime, source));
        }),
    );
    const file = new File([new Uint8Array([9, 9, 9])], "x.png", { type: "image/png" });
    fireEvent.paste(box(), { clipboardData: { files: [file], types: ["Files"] } });
    fireEvent.paste(box(), { clipboardData: { files: [file], types: ["Files"] } });
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    await act(async () => finish());
    await screen.findByText("Photo. I’ll read it tonight.");
    expect(spy).toHaveBeenCalledOnce();
  });

  it("a drop while a keep is on its way does nothing", async () => {
    const { engine, platform } = await open();
    let finish: () => void = () => undefined;
    const real = engine.keepText.bind(engine);
    vi.spyOn(engine, "keepText").mockImplementation(
      (text, source) =>
        new Promise((resolve) => {
          finish = () => resolve(real(text, source));
        }),
    );
    const spy = vi.spyOn(engine, "keepFiles");
    await userEvent.type(box(), "Buy milk{Enter}");
    act(() => platform.drop(["/tmp/a.jpg"]));
    expect(spy).not.toHaveBeenCalled();
    await act(async () => finish());
  });
});
