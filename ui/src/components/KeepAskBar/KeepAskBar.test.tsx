import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { defaultVerb, KeepAskBar } from "./KeepAskBar";

function setup(props: Partial<Parameters<typeof KeepAskBar>[0]> = {}) {
  const onKeep = vi.fn();
  const onAsk = vi.fn();
  render(<KeepAskBar placement="top" onKeep={onKeep} onAsk={onAsk} {...props} />);
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
    render(<KeepAskBar placement="top" onKeep={onKeep} onAsk={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "Keep something, or ask" });
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
