import { readFileSync } from "node:fs";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { OnlineView } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { EngineProvider } from "../../engine/context";
import { onlineViewOn } from "../../engine/mock/onlineAsk";
import { PlatformProvider } from "../../platform/context";
import type { Platform } from "../../platform/types";
import { onlineEngine } from "../../screens/ask/testing";
import { ToastProvider } from "../Toast/Toast";
import { askPlace } from "./askPlace";
import { defaultVerb, KeepAskBar } from "./KeepAskBar";

beforeEach(() => {
  askPlace.take();
});

function setup(
  props: Partial<Parameters<typeof KeepAskBar>[0]> = {},
  view: OnlineView | null = null,
) {
  const onKeep = vi.fn();
  const onAsk = vi.fn();
  render(
    <EngineProvider engine={onlineEngine(view)}>
      <KeepAskBar placement="top" onKeep={onKeep} onAsk={onAsk} {...props} />
    </EngineProvider>,
  );
  return { onKeep, onAsk, input: screen.getByRole("textbox", { name: "Keep something, or ask" }) };
}

describe("defaultVerb", () => {
  it("asks for a question mark or a question word", () => {
    for (const text of [
      "When is the dentist?",
      "who is Sam",
      "what did the doctor say",
      "Where are the keys",
      "why is the bin late",
      "How much was the invoice",
      "Is Leo allergic to nuts",
      "Are we out of milk",
      "Do I owe Sam anything",
      "Does Gran have my number",
      "Did I pay the permit",
      "Can I park there",
      "Will it rain Saturday",
      "Dentist on Thursday?",
    ]) {
      expect(defaultVerb(text), text).toBe("ask");
    }
  });

  it("keeps everything else", () => {
    for (const text of [
      "Ask Sam about Saturday",
      "Gran’s number ends 4471",
      "Whatever",
      "Island trip",
      "",
    ]) {
      expect(defaultVerb(text), text).toBe("keep");
    }
  });
});

describe("KeepAskBar", () => {
  it("asks on Enter for a question and keeps on Enter for anything else", async () => {
    const { onKeep, onAsk, input } = setup();
    await userEvent.type(input, "When is the dentist?{Enter}");
    expect(onAsk).toHaveBeenCalledWith("When is the dentist?");
    expect(onKeep).not.toHaveBeenCalled();
    await userEvent.type(input, "Ask Sam about Saturday{Enter}");
    expect(onKeep).toHaveBeenCalledWith("Ask Sam about Saturday");
  });

  it("Ctrl+Enter always keeps and Alt+Enter always asks", async () => {
    const { onKeep, onAsk, input } = setup();
    await userEvent.type(input, "When is the dentist?{Control>}{Enter}{/Control}");
    expect(onKeep).toHaveBeenCalledWith("When is the dentist?");
    await userEvent.type(input, "Gran’s number ends 4471{Alt>}{Enter}{/Alt}");
    expect(onAsk).toHaveBeenCalledWith("Gran’s number ends 4471");
  });

  it("shows both verbs once there is text, with the default as the primary", async () => {
    const { input } = setup();
    expect(screen.queryByRole("button", { name: /^Keep/ })).toBeNull();
    await userEvent.type(input, "When is the dentist?");
    const keep = screen.getByRole("button", { name: /^Keep/ });
    const ask = screen.getByRole("button", { name: /^Ask/ });
    expect(ask).toHaveAttribute("data-variant", "primary");
    expect(keep).toHaveAttribute("data-variant", "secondary");
    await userEvent.clear(input);
    await userEvent.type(input, "A new note");
    expect(screen.getByRole("button", { name: /^Keep/ })).toHaveAttribute(
      "data-variant",
      "primary",
    );
  });

  it("sends with the buttons too, and clears the field", async () => {
    const { onKeep, input } = setup();
    await userEvent.type(input, "A new note");
    await userEvent.click(screen.getByRole("button", { name: /^Keep/ }));
    expect(onKeep).toHaveBeenCalledWith("A new note");
    expect(input).toHaveValue("");
  });

  it("gives the words back when keeping failed", async () => {
    const onKeep = vi.fn().mockRejectedValue(new Error("no"));
    const { input } = setup({ onKeep });
    await userEvent.type(input, "A note I must not lose{Enter}");
    expect(onKeep).toHaveBeenCalled();
    expect(input).toHaveValue("A note I must not lose");
  });

  it("is a named search landmark", () => {
    setup();
    expect(document.querySelector("search")).toHaveAttribute("aria-label", "Keep or ask");
  });

  it("does nothing for an empty bar and Shift+Enter makes a new line", async () => {
    const { onKeep, onAsk, input } = setup();
    await userEvent.type(input, "{Enter}");
    expect(onKeep).not.toHaveBeenCalled();
    expect(onAsk).not.toHaveBeenCalled();
    await userEvent.type(input, "line one{Shift>}{Enter}{/Shift}line two");
    expect(input).toHaveValue("line one\nline two");
  });

  it("shows the shortcut on the wide bar only, and a shorter prompt on the narrow one", () => {
    const wide = setup({ shortcut: "Ctrl+Shift+Space" });
    expect(screen.getByText("Ctrl Shift Space")).toBeInTheDocument();
    expect(wide.input).toHaveAttribute("placeholder", "Keep something, or ask…");
  });

  it("uses the narrow prompt and no shortcut at the bottom", () => {
    const { input } = setup({ placement: "bottom", shortcut: "Ctrl+Shift+Space" });
    expect(input).toHaveAttribute("placeholder", "Keep or ask…");
    expect(screen.queryByText("Ctrl Shift Space")).toBeNull();
  });

  it("offers hold to talk and says it is listening", () => {
    const start = vi.fn();
    const stop = vi.fn();
    render(
      <KeepAskBar
        placement="top"
        onKeep={vi.fn()}
        onAsk={vi.fn()}
        onTalk={{ start, stop }}
        recordingSeconds={7}
      />,
    );
    expect(screen.getByRole("button", { name: "Hold to talk" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("status")).toHaveTextContent("Listening 0:07");
  });

  it("lets the keyboard hold to talk with Space", async () => {
    const start = vi.fn();
    const stop = vi.fn();
    render(
      <KeepAskBar placement="top" onKeep={vi.fn()} onAsk={vi.fn()} onTalk={{ start, stop }} />,
    );
    screen.getByRole("button", { name: "Hold to talk" }).focus();
    await userEvent.keyboard("{ }");
    expect(start).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
  });

  it("marks the microphone unavailable and says why, instead of offering it", () => {
    const start = vi.fn();
    render(
      <KeepAskBar
        placement="top"
        onKeep={vi.fn()}
        onAsk={vi.fn()}
        onTalk={{ start, stop: vi.fn() }}
        talkUnavailable="This window can’t record sound."
      />,
    );
    const mic = screen.getByRole("button", { name: "Hold to talk" });
    expect(mic).toHaveAttribute("aria-disabled", "true");
    expect(mic).toHaveAccessibleDescription("This window can’t record sound.");
    expect(mic).toHaveAttribute("title", "This window can’t record sound.");
  });

  it("does not mark the microphone when talking works", () => {
    render(
      <KeepAskBar
        placement="top"
        onKeep={vi.fn()}
        onAsk={vi.fn()}
        onTalk={{ start: vi.fn(), stop: vi.fn() }}
      />,
    );
    expect(screen.getByRole("button", { name: "Hold to talk" })).not.toHaveAttribute(
      "aria-disabled",
    );
  });
});

describe("KeepAskBar add a photo or file", () => {
  function withPicker(picked: string[], props: Partial<Parameters<typeof KeepAskBar>[0]> = {}) {
    const pickFiles = vi.fn(async () => picked);
    render(
      <PlatformProvider platform={{ pickFiles } as unknown as Platform}>
        <ToastProvider>
          <KeepAskBar placement="top" onKeep={vi.fn()} onAsk={vi.fn()} {...props} />
        </ToastProvider>
      </PlatformProvider>,
    );
    return pickFiles;
  }

  it("renders only when onFiles is set, named for what it does", () => {
    setup();
    expect(screen.queryByRole("button", { name: "Add a photo or file" })).toBeNull();
    withPicker([], { onFiles: vi.fn() });
    const button = screen.getByRole("button", { name: "Add a photo or file" });
    expect(button).toHaveAttribute("title", "Add a photo or file");
  });

  it("sits immediately before the microphone", () => {
    withPicker([], { onFiles: vi.fn(), onTalk: { start: vi.fn(), stop: vi.fn() } });
    const add = screen.getByRole("button", { name: "Add a photo or file" });
    expect(add.nextElementSibling).toBe(screen.getByRole("button", { name: "Hold to talk" }));
  });

  it("is a 44 px square, like the microphone", () => {
    const css = readFileSync("src/components/KeepAskBar/KeepAskBar.module.css", "utf8");
    expect(css).toMatch(
      /\.mic,\s*\.attach \{[^}]*width: var\(--size-touch\);[^}]*height: var\(--size-touch\);/,
    );
  });

  it("hands picked keys to onFiles and ignores an empty pick", async () => {
    const onFiles = vi.fn();
    const pick = withPicker(["k1", "k2"], { onFiles });
    await userEvent.click(screen.getByRole("button", { name: "Add a photo or file" }));
    expect(onFiles).toHaveBeenCalledWith(["k1", "k2"]);
    pick.mockResolvedValueOnce([]);
    onFiles.mockClear();
    await userEvent.click(screen.getByRole("button", { name: "Add a photo or file" }));
    expect(onFiles).not.toHaveBeenCalled();
  });

  it("says plainly when the engine cannot keep that kind of file", async () => {
    const onFiles = vi
      .fn()
      .mockRejectedValue(new EngineError("invalid", "this kind of content cannot be kept"));
    withPicker(["k1"], { onFiles });
    await userEvent.click(screen.getByRole("button", { name: "Add a photo or file" }));
    expect(await screen.findByText(/can’t keep that kind of file yet/)).toBeInTheDocument();
  });

  it("stays quiet about other failures, which the caller reports", async () => {
    const onFiles = vi.fn().mockRejectedValue(new EngineError("io", "disk full"));
    withPicker(["k1"], { onFiles });
    await userEvent.click(screen.getByRole("button", { name: "Add a photo or file" }));
    expect(screen.queryByText(/can’t keep that kind of file yet/)).toBeNull();
  });
});

describe("Answer this", () => {
  const typeQuestion = (input: HTMLElement) => userEvent.type(input, "When is the dentist");
  const group = () => screen.findByRole("radiogroup", { name: "Answer this" });

  it("is not there while Online answers is off or was never set up", async () => {
    const { input } = setup();
    await typeQuestion(input);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("offers online and this computer once there is something to ask, online chosen", async () => {
    const { input } = setup({}, onlineViewOn());
    await typeQuestion(input);
    const where = await group();
    expect(within(where).getByRole("radio", { name: "Online · api.example.com" })).toBeChecked();
    expect(within(where).getByRole("radio", { name: "This computer" })).not.toBeChecked();
    expect(screen.getByText(/go to your provider/)).toBeVisible();
  });

  it("shows nothing before there is text", async () => {
    setup({}, onlineViewOn());
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("is reached before the Ask button, so it can be set before submitting", async () => {
    const { input } = setup({}, onlineViewOn());
    await typeQuestion(input);
    await group();
    const radio = screen.getByRole("radio", { name: "Online · api.example.com" });
    const ask = screen.getByRole("button", { name: /^Ask/ });
    expect(radio.compareDocumentPosition(ask) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(radio.compareDocumentPosition(input) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("passes this computer for one question and goes back to online after it", async () => {
    const { onAsk, input } = setup({}, onlineViewOn());
    await typeQuestion(input);
    await group();
    await userEvent.click(screen.getByRole("radio", { name: "This computer" }));
    expect(screen.getByText(/Nothing leaves this computer/)).toBeVisible();
    await userEvent.click(input);
    await userEvent.keyboard("{Alt>}{Enter}{/Alt}");
    expect(onAsk).toHaveBeenCalledWith("When is the dentist");
    expect(askPlace.take()).toBe("thisComputer");
    await typeQuestion(input);
    expect(screen.getByRole("radio", { name: "Online · api.example.com" })).toBeChecked();
    await userEvent.keyboard("{Alt>}{Enter}{/Alt}");
    expect(askPlace.take()).toBe("auto");
  });

  it("says why online is not available while paused and offers the way to change it", async () => {
    const { input } = setup(
      {},
      onlineViewOn({ state: { type: "paused", pause: { type: "limit", until: "2026-12-01" } } }),
    );
    await typeQuestion(input);
    await group();
    expect(screen.getByRole("radio", { name: "Online · paused" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "This computer" })).toBeChecked();
    expect(screen.getByText(/paused until Tuesday 1 December/)).toBeVisible();
    expect(screen.getByRole("link", { name: "Change limit" })).toHaveAttribute(
      "href",
      "#/settings/online",
    );
  });
});
