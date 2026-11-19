import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CheckInInput } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { nearestWords } from "../../journal/feelings";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  cleanup();
  window.location.hash = "";
});

const NOW = "2026-11-19T21:20:00Z";

async function open(hash = "#/journal/check-in") {
  const engine = createMockEngine({ now: NOW });
  const view = renderApp({ hash, engine });
  await screen.findByRole("heading", { level: 1, name: "How are you feeling?" });
  return { ...view, engine };
}

/** jsdom has no layout: give the field a 200 by 200 box at the origin. */
function field() {
  const group = screen.getByRole("radiogroup", {
    name: "How you feel, by energy and how pleasant it is",
  });
  group.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 200, height: 200, right: 200, bottom: 200, x: 0, y: 0 }) as DOMRect;
  return group;
}

const chips = () =>
  within(screen.getByRole("group", { name: /^Words for / })).getAllByRole("button");
const chip = (name: string) =>
  within(screen.getByRole("group", { name: /^Words for / })).getByRole("button", { name });

describe("the check-in screen", () => {
  it("says what it is and offers the field and the list", async () => {
    await open();
    expect(
      screen.getByText("Pick a spot, then the words that fit. There are no wrong answers."),
    ).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByRole("link", { name: /Journal/ })).toHaveAttribute(
      "href",
      "#/journal",
    );
    expect(screen.getByRole("button", { name: "Show all words as a list" })).toBeInTheDocument();
    expect(
      screen.getByText("Pick a spot in the field, or show all words as a list."),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the words of the part with the nearest first", async () => {
    await open();
    fireEvent.pointerDown(field(), { clientX: 190, clientY: 190, pointerId: 1 });
    expect(screen.getByRole("heading", { name: "Words for Settled" })).toBeInTheDocument();
    expect(screen.getByText("Choose up to three.")).toBeInTheDocument();
    const shown = chips().map((c) => c.textContent);
    const expected = nearestWords("settled", { pleasant: 0.9, energy: -0.9 }).map((w) => w.word);
    expect(shown.slice(0, 12)).toEqual(expected);
    expect(shown[0]).toBe("rested");
    // A point near the middle brings the mild words first.
    fireEvent.pointerDown(field(), { clientX: 110, clientY: 110, pointerId: 1 });
    expect(chips()[0]).toHaveTextContent("relaxed");
  });

  it("takes up to three words and no more", async () => {
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    for (const word of ["calm", "content", "grateful", "safe"]) {
      await userEvent.click(chip(word));
    }
    expect(chip("calm")).toHaveAttribute("aria-pressed", "true");
    expect(chip("content")).toHaveAttribute("aria-pressed", "true");
    expect(chip("grateful")).toHaveAttribute("aria-pressed", "true");
    expect(chip("safe")).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(chip("calm"));
    await userEvent.click(chip("safe"));
    expect(chip("calm")).toHaveAttribute("aria-pressed", "false");
    expect(chip("safe")).toHaveAttribute("aria-pressed", "true");
  });

  it("shows a word's meaning as text when it has focus or is hovered", async () => {
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    act(() => chip("grateful").focus());
    expect(screen.getByText(/Thankful for something or someone/)).toHaveTextContent(
      "grateful: Thankful for something or someone",
    );
    await userEvent.hover(chip("calm"));
    expect(screen.getByText(/Quiet inside/)).toHaveTextContent("calm: Quiet inside");
  });

  it("More words adds the eight, and Fewer words takes them away", async () => {
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(chips()).toHaveLength(12);
    await userEvent.click(screen.getByRole("button", { name: "More words" }));
    expect(chips()).toHaveLength(20);
    expect(chip("burnt out")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Fewer words" }));
    expect(chips()).toHaveLength(12);
  });

  it("changing part drops words that belong to the part left", async () => {
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    await userEvent.click(chip("calm"));
    await userEvent.click(screen.getByRole("radio", { name: "Low" }));
    await userEvent.click(chip("tired"));
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    expect(chip("calm")).toHaveAttribute("aria-pressed", "false");
  });

  it("saves the point, words, line and urge", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepCheckIn");
    fireEvent.pointerDown(field(), { clientX: 150, clientY: 150, pointerId: 1 });
    await userEvent.click(chip("relieved"));
    await userEvent.click(chip("content"));
    await userEvent.type(
      screen.getByLabelText("What’s going on? (optional)"),
      "Gran’s scan came back clear.",
    );
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.type(
      screen.getByLabelText("Did you feel like doing anything? (optional)"),
      "Ring her",
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    const expected: CheckInInput = {
      feeling: {
        part: "settled",
        point: { pleasant: 0.5, energy: -0.5 },
        words: ["relieved", "content"],
      },
      line: "Gran’s scan came back clear.",
      urge: "Ring her",
      replaces: null,
    };
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expected));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Saved. Relieved, content." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/It’s on today’s journal/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Something that might help ›" })).toBeNull();
  });

  it("saves with a part only, and leaves the lines empty", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepCheckIn");
    await userEvent.click(screen.getByRole("radio", { name: "Lit up" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        feeling: { part: "litUp", point: { pleasant: 0.5, energy: 0.5 }, words: [] },
        line: null,
        urge: null,
        replaces: null,
      }),
    );
    expect(await screen.findByRole("heading", { level: 1, name: "Saved." })).toBeInTheDocument();
  });

  it("starts with a part when the address names one", async () => {
    await open("#/journal/check-in?part=litUp");
    expect(screen.getByRole("radio", { name: "Lit up" })).toBeChecked();
    expect(screen.getByRole("heading", { name: "Words for Lit up" })).toBeInTheDocument();
  });

  it("ignores a part it does not know", async () => {
    await open("#/journal/check-in?part=blue");
    for (const radio of screen.getAllByRole("radio")) expect(radio).not.toBeChecked();
  });
});

describe("all words as a list", () => {
  it("swaps the field for words grouped by part, and back", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Show all words as a list" }));
    expect(screen.queryByRole("radiogroup")).toBeNull();
    for (const part of ["Wound up", "Lit up", "Low", "Settled"]) {
      const group = screen.getByRole("group", { name: part });
      expect(within(group).getAllByRole("checkbox")).toHaveLength(12);
    }
    await userEvent.click(screen.getByRole("button", { name: "Show the field" }));
    expect(screen.getByRole("radiogroup")).toBeInTheDocument();
  });

  it("takes the part of the first word chosen, and up to three from any part", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepCheckIn");
    await userEvent.click(screen.getByRole("button", { name: "Show all words as a list" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "proud" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "drained" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "calm" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "safe" }));
    expect(screen.getByRole("checkbox", { name: "safe" })).not.toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        feeling: {
          part: "litUp",
          point: { pleasant: 0.5, energy: 0.5 },
          words: ["proud", "drained", "calm"],
        },
        line: null,
        urge: null,
        replaces: null,
      }),
    );
  });

  it("will not save with nothing chosen", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Show all words as a list" }));
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows a word's meaning as text on focus", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Show all words as a list" }));
    act(() => screen.getByRole("checkbox", { name: "numb" }).focus());
    expect(screen.getByText(/Cut off from feeling/)).toHaveTextContent(
      "numb: Cut off from feeling",
    );
  });

  it("keeps the words chosen when it goes back to the field", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Show all words as a list" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "tired" }));
    await userEvent.click(screen.getByRole("button", { name: "Show the field" }));
    expect(screen.getByRole("radio", { name: "Low" })).toBeChecked();
    expect(chip("tired")).toHaveAttribute("aria-pressed", "true");
  });
});

describe("changing a check-in", () => {
  it("opens with its choice made and saves as a replacement", async () => {
    const { engine } = await open("#/journal/check-in?change=j-ci-tired");
    const spy = vi.spyOn(engine, "keepCheckIn");
    await waitFor(() => expect(screen.getByRole("radio", { name: "Low" })).toBeChecked());
    expect(chip("tired")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("What’s going on? (optional)")).toHaveValue("Bad night.");
    await userEvent.click(chip("drained"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const sent = spy.mock.calls[0]?.[0];
    expect(sent?.replaces).toBe("j-ci-tired");
    expect(sent?.feeling.words).toEqual(["tired", "drained"]);
    expect(sent?.feeling.part).toBe("low");
    expect(sent?.line).toBe("Bad night.");
  });

  it("Change on the saved page goes back to the choices and replaces what was just saved", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "keepCheckIn");
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    await userEvent.click(chip("calm"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("heading", { level: 1, name: "Saved. Calm." });
    const first = await spy.mock.results[0]?.value;
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(chip("calm")).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(chip("calm"));
    await userEvent.click(chip("safe"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    expect(spy.mock.calls[1]?.[0].replaces).toBe(first.id);
  });
});

describe("something that might help", () => {
  async function saveIn(part: "Low" | "Wound up" | "Lit up" | "Settled") {
    const view = await open();
    await userEvent.click(screen.getByRole("radio", { name: part }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("heading", { level: 1, name: "Saved." });
    return view;
  }

  it.each(["Low", "Wound up"] as const)("is offered after %s", async (part) => {
    await saveIn(part);
    expect(screen.getByRole("button", { name: "Something that might help ›" })).toBeInTheDocument();
  });

  it.each(["Lit up", "Settled"] as const)("is not offered after %s", async (part) => {
    await saveIn(part);
    expect(screen.queryByText(/Something that might help/)).toBeNull();
    expect(screen.getByRole("button", { name: "Back to the journal" })).toBeInTheDocument();
  });

  it("shows four kinds of two ideas and the safety line", async () => {
    await saveIn("Low");
    await userEvent.click(screen.getByRole("button", { name: "Something that might help ›" }));
    expect(
      screen.getByText("Some ideas people find useful. Try one if you like."),
    ).toBeInTheDocument();
    for (const kind of ["Move", "Slow down", "Think it through", "Reach someone"]) {
      const group = screen.getByRole("group", { name: kind });
      expect(within(group).getAllByRole("radio")).toHaveLength(2);
    }
    expect(screen.getAllByRole("radio")).toHaveLength(8);
    expect(
      screen.getByText(
        "If you feel unsafe, or might hurt yourself, call your local emergency number or someone you trust now.",
      ),
    ).toBeInTheDocument();
  });

  it("needs a choice before it notes anything, then notes the idea once chosen", async () => {
    const { engine } = await saveIn("Low");
    const spy = vi.spyOn(engine, "markTried");
    await userEvent.click(screen.getByRole("button", { name: "Something that might help ›" }));
    expect(screen.getByText("Pick one above first.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "I tried this" }));
    expect(spy).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("radio", { name: "Name five things you can see." }));
    expect(screen.queryByText("Pick one above first.")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "I tried this" }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0]?.[1]).toBe("journal.help.idea.slow.name");
    expect(await screen.findByText("Noted.")).toBeInTheDocument();
    expect(screen.queryByText(/worked/i)).toBeNull();
  });

  it("only one idea can be chosen at a time", async () => {
    await saveIn("Wound up");
    await userEvent.click(screen.getByRole("button", { name: "Something that might help ›" }));
    await userEvent.click(screen.getByRole("radio", { name: "Name five things you can see." }));
    await userEvent.click(
      screen.getByRole("radio", { name: "Walk to the end of the road and back." }),
    );
    expect(
      screen.getAllByRole("radio").filter((r) => (r as HTMLInputElement).checked),
    ).toHaveLength(1);
  });

  it("Back to the journal goes to the journal", async () => {
    await saveIn("Low");
    await userEvent.click(screen.getByRole("button", { name: "Back to the journal" }));
    expect(window.location.hash).toBe("#/journal");
  });
});
