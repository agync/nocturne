import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { type FakePlatform, fakePlatform, renderApp, setViewport } from "../../test/render";
import { CLOSE_AFTER_MS } from "./index";

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
    const { close } = await open();
    await userEvent.type(box(), "Gran’s new number ends 4471.{Enter}");
    expect(await screen.findByText(/You already kept this note on/)).toHaveTextContent(
      "Nothing new to add.",
    );
    expect(screen.queryByRole("button", { name: "Keep anyway" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open it" }));
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
  const hold = async () => {
    box().focus();
    fireEvent.keyDown(box(), { key: " " });
    await screen.findByText("Listening");
  };

  it("hold Space on empty records with a meter and timer, release keeps a voice note", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepRecording");
    await hold();
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
