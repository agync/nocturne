import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import type { AskDelta, MemoryView } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { askSession } from "./session";

type Step = Partial<AskDelta> & Pick<AskDelta, "phase">;

/** The real mock for everything except the answer, which a test can script step by step. */
class Scripted extends MockEngine {
  steps: Step[] = [];
  /** Steps after this many are held back until `release()`. */
  holdAfter = Number.POSITIVE_INFINITY;
  private held: Step[] = [];
  asked: string[] = [];
  refuse = false;

  override async ask(question: string): Promise<string> {
    if (this.refuse) throw new Error("The engine said no");
    this.asked.push(question);
    if (this.steps.length === 0) return super.ask(question);
    const askId = `scripted-${this.asked.length}`;
    this.steps.forEach((step, i) => {
      if (i >= this.holdAfter) {
        this.held.push(step);
        return;
      }
      setTimeout(() => this.send(askId, step), 5 * (i + 1));
    });
    this.heldId = askId;
    return askId;
  }

  heldId = "";

  release() {
    for (const step of this.held) this.send(this.heldId, step);
    this.held = [];
  }

  private send(askId: string, step: Step) {
    const delta: AskDelta = {
      askId,
      textDelta: null,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
      plain: false,
      ...step,
    };
    // emit is private on the mock; a test engine may reach it.
    (this as unknown as { emit(name: string, delta: AskDelta): void }).emit("ask-delta", delta);
  }
}

const memory = async (engine: MockEngine, id: string): Promise<MemoryView> => {
  const all = await engine.listMemories({
    entityId: null,
    itemId: null,
    status: null,
    query: null,
    limit: null,
  });
  return all.find((m) => m.id === id) as MemoryView;
};

/** "Leo swims **on Mondays** at 17:30 with **Coach Sam**." as two memories, streamed. */
async function twoMemoryAnswer(engine: Scripted, hold = Number.POSITIVE_INFINITY) {
  const swim = await memory(engine, "mem-swim");
  const coach = await memory(engine, "mem-coach");
  engine.holdAfter = hold;
  engine.steps = [
    { phase: "searching" },
    {
      phase: "reading",
      found: 2,
      receipts: [...swim.receipts, ...coach.receipts],
      memoryIds: [swim.id, coach.id],
    },
    { phase: "streaming", textDelta: "Leo swims **on Mondays** " },
    { phase: "streaming", textDelta: "at 17:30 with **Coach " },
    { phase: "streaming", textDelta: "Sam**." },
    { phase: "done", memoryIds: [swim.id, coach.id] },
  ];
}

beforeEach(() => {
  askSession.reset();
  setViewport(1280);
});
afterEach(() => {
  askSession.reset();
  window.location.hash = "";
  askRequests.take();
});

const open = async (
  options: {
    question?: string;
    engine?: Scripted;
    platform?: ReturnType<typeof fakePlatform>;
  } = {},
) => {
  const engine = options.engine ?? new Scripted();
  if (options.question) askRequests.request(options.question);
  const platform = options.platform ?? fakePlatform();
  renderApp({ hash: "#/ask", engine, platform });
  return { engine, platform };
};

/** An answer paragraph by its words, whichever phrases in it are bold. */
const plain =
  (text: string) =>
  (_content: string, element: Element | null): boolean =>
    element?.tagName === "P" && element.textContent === text;

const heading = (name: string | RegExp) => screen.findByRole("heading", { level: 1, name });
const announcer = () =>
  screen
    .getAllByRole("status")
    .find((el) => el.classList.contains("sr-only") && el.getAttribute("aria-atomic") === "true");

describe("the Ask page when nothing has been asked", () => {
  it("offers examples instead of a blank page, and asking one works", async () => {
    const { engine } = await open();
    await heading("Ask about anything you’ve kept.");
    await userEvent.click(screen.getByRole("button", { name: "What is Gran’s new number?" }));
    expect(await heading("“What is Gran’s new number?”")).toBeInTheDocument();
    expect(engine.asked).toEqual(["What is Gran’s new number?"]);
    expect(await screen.findByText(plain("Gran’s new number ends 4471."))).toBeInTheDocument();
  });
});

describe("a question from the bar", () => {
  it("is taken on arrival: the question becomes the headline and the request is spent", async () => {
    const { engine } = await open({ question: "What is Gran’s new number?" });
    expect(await heading("“What is Gran’s new number?”")).toBeInTheDocument();
    expect(engine.asked).toEqual(["What is Gran’s new number?"]);
    expect(askRequests.peek()).toBeNull();
    expect(screen.getByText("You asked")).toBeInTheDocument();
  });

  it("a later question stacks the first under Earlier and replaces the headline", async () => {
    const { engine } = await open({ question: "What is Gran’s new number?" });
    await screen.findByRole("button", { name: "That’s wrong" });
    await act(async () => askRequests.request("What did we decide about half term?"));
    expect(await heading("“What did we decide about half term?”")).toBeInTheDocument();
    const earlier = screen.getByRole("region", { name: "Earlier" });
    expect(within(earlier).getByText("“What is Gran’s new number?”")).toBeInTheDocument();
    expect(within(earlier).getByText(plain("Gran’s new number ends 4471."))).toBeInTheDocument();
    expect(engine.asked).toHaveLength(2);
  });

  it("the follow-up field asks too", async () => {
    const { engine } = await open({ question: "What is Gran’s new number?" });
    await screen.findByText(plain("Gran’s new number ends 4471."));
    await userEvent.type(
      screen.getByRole("textbox", { name: "Ask a follow-up" }),
      "half term{Enter}",
    );
    await waitFor(() => expect(engine.asked).toContain("half term"));
    expect(await heading("“half term”")).toBeInTheDocument();
  });
});

describe("the answer", () => {
  it("streams, shows receipts before the sentence ends and announces once on completion", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine, 4);
    await open({ engine, question: "When is Leo’s swimming?" });
    // Receipts are there while the answer is still being written.
    expect(
      await screen.findAllByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    ).toHaveLength(2);
    await screen.findByText(/on Mondays/);
    expect(screen.queryByText(/Sam/, { selector: "p" })).toBeNull();
    // The text itself is never in a live region while it streams.
    expect(announcer()).toHaveTextContent("");
    await act(async () => engine.release());
    await waitFor(() =>
      expect(announcer()).toHaveTextContent(
        "Answer ready. Leo swims on Mondays at 17:30 with Coach Sam.",
      ),
    );
    expect(document.querySelector("p[class*=answer]")?.closest("[aria-live]")).toBeNull();
  });

  it("maps the nth bold span to the nth memory and outlines its receipt on hover and on focus", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine);
    await open({ engine, question: "When is Leo’s swimming?" });
    const mondays = await screen.findByRole("button", {
      name: "on Mondays. Show where this came from.",
    });
    const coach = await screen.findByRole("button", {
      name: "Coach Sam. Show where this came from.",
    });
    await waitFor(() => expect(screen.getAllByRole("list", { name: /^Where “/ })).toHaveLength(2));
    const groups = screen.getAllByRole("list", { name: /^Where “/ });
    const cardOf = (group: HTMLElement) => group.querySelector("li") as HTMLElement;
    const [first, second] = groups as [HTMLElement, HTMLElement];
    expect(first).toHaveAttribute("aria-label", expect.stringContaining("swimming is on Mondays"));
    await userEvent.hover(mondays);
    expect(cardOf(first)).toHaveAttribute("data-active", "true");
    expect(cardOf(second)).toHaveAttribute("data-active", "false");
    await userEvent.unhover(mondays);
    expect(cardOf(first)).toHaveAttribute("data-active", "false");
    act(() => coach.focus());
    expect(cardOf(second)).toHaveAttribute("data-active", "true");
    act(() => coach.blur());
    expect(cardOf(second)).toHaveAttribute("data-active", "false");
  });

  it("pressing a bold span moves to its receipt", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine);
    await open({ engine, question: "When is Leo’s swimming?" });
    await userEvent.click(
      await screen.findByRole("button", { name: "Coach Sam. Show where this came from." }),
    );
    const groups = await screen.findAllByRole("list", { name: /^Where “/ });
    expect(
      within(groups[1] as HTMLElement).getByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    ).toHaveFocus();
  });

  it("a receipt opens its original", async () => {
    await open({ question: "What is Gran’s new number?" });
    await userEvent.click(await screen.findByRole("button", { name: /Note · / }));
    expect(window.location.hash).toBe("#/items/item-gran");
  });

  it("offers no Read aloud until that edition exists", async () => {
    await open({ question: "What is Gran’s new number?" });
    await screen.findByText(plain("Gran’s new number ends 4471."));
    expect(screen.queryByRole("button", { name: /Read aloud/ })).toBeNull();
  });

  it("says how many things it answered from, and that nothing was sent only when measured", async () => {
    await open({
      question: "What is Gran’s new number?",
      engine: new Scripted({ sentMeasured: true }),
    });
    expect(
      await screen.findByText("Answered from 1 thing you kept · nothing sent to the internet"),
    ).toBeInTheDocument();
  });

  it("claims nothing about the internet while that is not measured", async () => {
    await open({ question: "What is Gran’s new number?" });
    expect(await screen.findByText("Answered from 1 thing you kept")).toBeInTheDocument();
    expect(screen.queryByText(/sent to the internet/)).toBeNull();
  });
});

describe("a slow first question", () => {
  it("says it is waking, how long, what it found, and lets the person see the originals now", async () => {
    const engine = new Scripted();
    const swim = await memory(engine, "mem-swim");
    const coach = await memory(engine, "mem-coach");
    engine.holdAfter = 2;
    engine.steps = [
      { phase: "waking", etaSeconds: 20 },
      {
        phase: "waking",
        found: 2,
        receipts: [...swim.receipts, ...coach.receipts],
        memoryIds: [swim.id, coach.id],
      },
      { phase: "searching" },
      { phase: "done", textDelta: "Mondays.", memoryIds: [swim.id] },
    ];
    await open({ engine, question: "When is the dentist?" });
    expect(await screen.findByText("Waking up, first question today")).toBeInTheDocument();
    expect(screen.getByText(/This takes about 20 seconds/)).toHaveTextContent(
      "Later questions are quick.",
    );
    expect(await screen.findByText(/Found 2 things you kept about this/)).toBeInTheDocument();
    expect(screen.getByText(/Reading them…/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    ).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Show the 2 originals now" }));
    expect(
      screen.getAllByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    ).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /originals now/ })).toBeNull();
  });
});

describe("not sure", () => {
  it("shows the near matches and a link to search for the whole question", async () => {
    await open({ question: "When is my appointment?" });
    expect(await screen.findByText(/Not sure\. Nothing says that outright/)).toBeVisible();
    expect(screen.getAllByText(/Your dentist appointment is/)).toHaveLength(2);
    expect(screen.getByRole("link", { name: "Search everything you’ve kept" })).toHaveAttribute(
      "href",
      "#/memories?q=When+is+my+appointment%3F",
    );
    await waitFor(() => expect(announcer()).toHaveTextContent("Not sure. 2 things come close."));
  });
});

describe("nothing found", () => {
  it("says so, offers to keep a photo and to search what was let go", async () => {
    const platform = fakePlatform();
    platform.pickFiles = vi.fn(async () => ["/tmp/passport.jpg"]);
    const { engine } = await open({ question: "passport", platform });
    expect(await screen.findByText("Nothing you’ve kept answers that.")).toBeVisible();
    expect(screen.getByText(/Nocturne only answers from what you keep/)).toBeVisible();
    const spy = vi.spyOn(engine, "keepFiles");
    await userEvent.click(screen.getByRole("button", { name: "Keep a photo now" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(["/tmp/passport.jpg"], "drop"));
    await userEvent.click(screen.getByRole("button", { name: "Search what was let go" }));
    expect(window.location.hash).toBe("#/fading?show=letGo");
  });
});

describe("before Nocturne is ready", () => {
  it("says it is still getting ready and offers to keep the question as a note", async () => {
    const engine = new Scripted();
    engine.steps = [{ phase: "unavailable" }];
    await open({ engine, question: "When is the dentist?" });
    expect(
      await screen.findByText(
        "Nocturne is still getting ready, so it can’t answer yet. What you keep is safe.",
      ),
    ).toBeVisible();
    const spy = vi.spyOn(engine, "keepText");
    await userEvent.click(screen.getByRole("button", { name: "Keep it as a note" }));
    expect(spy).toHaveBeenCalledWith("When is the dentist?", "quickCapture");
    await waitFor(() =>
      expect(
        screen.getAllByText("Kept as a note. Nocturne will answer when it is ready."),
      ).not.toHaveLength(0),
    );
    expect(screen.queryByRole("button", { name: "Keep it as a note" })).toBeNull();
  });

  it("says the same when the engine refuses the question", async () => {
    const engine = new Scripted();
    engine.refuse = true;
    await open({ engine, question: "When is the dentist?" });
    expect(await screen.findByText(/still getting ready/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Keep it as a note" })).toBeVisible();
  });
});

describe("what to do about an answer", () => {
  it("That’s wrong opens an inline correction for the one memory and saves it", async () => {
    const { engine } = await open({ question: "What is Gran’s new number?" });
    const spy = vi.spyOn(engine, "correctMemory");
    await screen.findByText(plain("Gran’s new number ends 4471."));
    await userEvent.click(await screen.findByRole("button", { name: "That’s wrong" }));
    const field = await screen.findByRole("textbox", {
      name: "What is right? Write it as one sentence.",
    });
    expect(field).toHaveFocus();
    expect(field).toHaveValue("Gran’s new number ends 4471.");
    await userEvent.clear(field);
    await userEvent.type(field, "Gran’s new number ends 5582.");
    await userEvent.click(screen.getByRole("button", { name: "Save the correction" }));
    expect(spy).toHaveBeenCalledWith("mem-gran", "Gran’s new number ends 5582.");
    expect(await screen.findByText("Corrected. The old answer is kept as history.")).toBeVisible();
    expect(screen.queryByRole("textbox", { name: /What is right/ })).toBeNull();
  });

  it("Escape closes the correction and returns to the button", async () => {
    await open({ question: "What is Gran’s new number?" });
    const wrong = await screen.findByRole("button", { name: "That’s wrong" });
    await userEvent.click(wrong);
    await screen.findByRole("textbox", { name: /What is right/ });
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: /What is right/ })).toBeNull();
    expect(wrong).toHaveFocus();
  });

  it("Keep forever on one memory does it at once and says so", async () => {
    const { engine } = await open({ question: "What did we decide about half term?" });
    const spy = vi.spyOn(engine, "setKeepForever");
    await userEvent.click(await screen.findByRole("button", { name: "Keep forever" }));
    expect(spy).toHaveBeenCalledWith("mem-lake", true);
    expect(await screen.findByText("Kept forever. It will not fade.")).toBeVisible();
    expect(await screen.findByRole("button", { name: "Kept forever" })).toBeDisabled();
  });

  it("with several memories, each action asks which one first", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine);
    await open({ engine, question: "When is Leo’s swimming?" });
    const spy = vi.spyOn(engine, "setKeepForever");
    const keep = await screen.findByRole("button", { name: "Keep forever" });
    await waitFor(() => expect(keep).toBeEnabled());
    await userEvent.click(keep);
    const chooser = await screen.findByRole("group", { name: "Which one?" });
    expect(within(chooser).getAllByRole("button")).toHaveLength(3);
    expect(within(chooser).getAllByRole("button")[0]).toHaveFocus();
    await userEvent.click(within(chooser).getByRole("button", { name: "All of them" }));
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it("That’s wrong with several memories lists them without an All option", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine);
    await open({ engine, question: "When is Leo’s swimming?" });
    const wrong = await screen.findByRole("button", { name: "That’s wrong" });
    await waitFor(() => expect(wrong).toBeEnabled());
    await userEvent.click(wrong);
    const chooser = await screen.findByRole("group", { name: "Which one?" });
    expect(within(chooser).queryByRole("button", { name: "All of them" })).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(wrong).toHaveFocus();
  });

  it("Delete this… opens the shared forget dialog for that memory", async () => {
    await open({ question: "What is Gran’s new number?" });
    await userEvent.click(await screen.findByRole("button", { name: "Delete this…" }));
    expect(await screen.findByRole("dialog", { name: "Forget 1 memory?" })).toBeVisible();
  });
});

describe("coming back to an answer", () => {
  it("a receipt round trip finds the answer and Earlier where they were, without asking again", async () => {
    const { engine } = await open({ question: "What is Gran’s new number?" });
    await screen.findByRole("button", { name: "That’s wrong" });
    await act(async () => askRequests.request("What did we decide about half term?"));
    await heading("“What did we decide about half term?”");
    await screen.findByRole("button", { name: "That’s wrong" });
    const article = document.querySelector("article") as HTMLElement;
    await userEvent.click(
      await within(article).findByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    );
    expect(window.location.hash).toMatch(/^#\/items\//);
    act(() => {
      window.location.hash = "#/ask";
    });
    expect(await heading("“What did we decide about half term?”")).toBeInTheDocument();
    expect(await screen.findByText(/You decided to book the/)).toBeInTheDocument();
    const earlier = screen.getByRole("region", { name: "Earlier" });
    expect(within(earlier).getByText(plain("Gran’s new number ends 4471."))).toBeInTheDocument();
    expect(engine.asked).toHaveLength(2);
    expect(
      within(document.querySelector("article") as HTMLElement).getByRole("button", {
        name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
      }),
    ).toBeInTheDocument();
  });

  it("an answer still being written is not restored", async () => {
    const engine = new Scripted();
    await twoMemoryAnswer(engine, 2);
    await open({ engine, question: "When is Leo’s swimming?" });
    await screen.findAllByRole("button", {
      name: /(Note|Photo|Voice note|Document|Message you pasted|Web page you pasted) · /,
    });
    act(() => {
      window.location.hash = "#/today";
    });
    await screen.findByRole("heading", { level: 1, name: /need/ });
    act(() => {
      window.location.hash = "#/ask";
    });
    expect(await heading("Ask about anything you’ve kept.")).toBeInTheDocument();
  });
});

describe("the page after asking", () => {
  it("an example moves focus to the new headline", async () => {
    await open();
    await userEvent.click(
      await screen.findByRole("button", { name: "What is Gran’s new number?" }),
    );
    const h1 = await heading("“What is Gran’s new number?”");
    expect(h1).toHaveFocus();
  });

  it("Earlier has no headings, so the question is the first heading on the page", async () => {
    await open({ question: "What is Gran’s new number?" });
    await screen.findByRole("button", { name: "That’s wrong" });
    await act(async () => askRequests.request("What did we decide about half term?"));
    await heading("“What did we decide about half term?”");
    const headings = screen.getAllByRole("heading").filter((h) => h.closest("main"));
    expect(headings[0]).toHaveTextContent("What did we decide about half term?");
    expect(screen.getByRole("region", { name: "Earlier" })).toBeInTheDocument();
  });

  it("an earlier not-sure answer says how many came close", async () => {
    await open({ question: "When is my appointment?" });
    await screen.findByText(/Not sure\. Nothing says/);
    await act(async () => askRequests.request("What is Gran’s new number?"));
    await heading("“What is Gran’s new number?”");
    const earlier = screen.getByRole("region", { name: "Earlier" });
    expect(within(earlier).getByText("Not sure. 2 things come close.")).toBeInTheDocument();
  });
});

describe("a correction", () => {
  it("leaves the old answer struck through with the right sentence beside it", async () => {
    await open({ question: "What is Gran’s new number?" });
    await userEvent.click(await screen.findByRole("button", { name: "That’s wrong" }));
    const field = await screen.findByRole("textbox", { name: /What is right/ });
    await userEvent.clear(field);
    await userEvent.type(field, "Gran’s new number ends 5582.");
    await userEvent.click(screen.getByRole("button", { name: "Save the correction" }));
    expect(await screen.findByText("Corrected: Gran’s new number ends 5582.")).toBeVisible();
    expect(document.querySelector("p[data-corrected='true']")).toHaveTextContent(
      "Gran’s new number ends 4471.",
    );
  });
});
