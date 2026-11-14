import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { askRequests } from "../../app/ask-request";
import { askPlace } from "../../components/KeepAskBar/askPlace";
import type { AnsweredBy, LeftOut, LocalReason, OnlineView } from "../../engine/bindings";
import { onlineViewOn } from "../../engine/mock/onlineAsk";
import { renderApp, setViewport } from "../../test/render";
import { askSession } from "./session";
import { oneMemoryAnswer, onlineEngine, ScriptedAnswers, type Step } from "./testing";

const HOST = "api.example.com";

beforeEach(() => {
  askSession.reset();
  setViewport(1280);
  askPlace.take();
});
afterEach(() => {
  askSession.reset();
  window.location.hash = "";
  askRequests.take();
});

/** Online answers in the given state on a computer that answers with the given steps. */
async function open(
  view: OnlineView | null,
  answer: (engine: ScriptedAnswers) => Promise<Step[]> | Step[],
  question: string | null = "When is Leo’s swimming?",
) {
  const engine = new ScriptedAnswers();
  const online = onlineEngine(view);
  Object.assign(engine, { online: online.online });
  engine.steps = await answer(engine);
  if (question) askRequests.request(question);
  renderApp({ hash: "#/ask", engine });
  return engine;
}

const finished =
  (by: AnsweredBy | null, extra: Partial<Step> = {}) =>
  async (e: ScriptedAnswers) =>
    oneMemoryAnswer(e, { phase: "done", answeredBy: by, sendId: 7, ...extra });

const announcer = () =>
  screen
    .getAllByRole("status")
    .find((el) => el.classList.contains("sr-only") && el.getAttribute("aria-atomic") === "true");

describe("the mark on an answer", () => {
  it("says an online answer was written by your provider and links to what was sent", async () => {
    await open(onlineViewOn(), finished({ type: "online", host: HOST }));
    const mark = await screen.findByText(
      "Answered online by your provider (api.example.com) from 1 thing you kept",
    );
    const link = within(mark.parentElement as HTMLElement).getByRole("link", {
      name: "See what was sent",
    });
    expect(link).toHaveAttribute("href", "#/settings/online/sent?send=7");
  });

  const REASONS: Array<[LocalReason, string]> = [
    ["chosen", "as you chose for this question"],
    ["noInternet", "no internet"],
    ["couldntReach", "couldn’t reach your provider (api.example.com)"],
    ["slow", "your provider was slow"],
    ["limit", "your monthly limit is reached"],
    ["noCredit", "your provider says there’s no credit left"],
    ["keyRefused", "your provider refused the key"],
    ["keptHere", "these stay on this computer"],
    ["problem", "online answers needs a look"],
    ["downloading", "online answers wait while a download finishes"],
    ["turnedOff", "online answers was turned off"],
  ];

  it.each(REASONS)("says why on this computer: %s", async (reason, words) => {
    await open(onlineViewOn(), finished({ type: "computer", reason }));
    expect(
      await screen.findByText(`Answered on this computer from 1 thing you kept · ${words}`),
    ).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "See what was sent" })).toBeNull();
  });

  it("says this computer answered when your provider was not sure, and links to what was sent", async () => {
    await open(onlineViewOn(), finished({ type: "computer", reason: "notSureOnline" }));
    const mark = await screen.findByText(
      "Answered on this computer from 1 thing you kept · your provider wasn’t sure",
    );
    const link = within(mark.parentElement as HTMLElement).getByRole("link", {
      name: "See what was sent",
    });
    expect(link).toHaveAttribute("href", "#/settings/online/sent?send=7");
  });

  it("is read out first, before the answer", async () => {
    await open(onlineViewOn(), finished({ type: "online", host: HOST }));
    await waitFor(() =>
      expect(announcer()).toHaveTextContent(
        "Answer ready. Answered online by your provider, api.example.com. Leo swims on Mondays.",
      ),
    );
  });

  it("sits above the words 'You asked' in the page", async () => {
    await open(onlineViewOn(), finished({ type: "online", host: HOST }));
    const mark = await screen.findByText(/^Answered online by your provider/);
    const asked = screen.getByText("You asked");
    expect(mark.compareDocumentPosition(asked) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps today's line, with no mark, when Online answers is off", async () => {
    await open(null, finished(null));
    expect(await screen.findByText("Answered from 1 thing you kept")).toBeInTheDocument();
    expect(screen.queryByText(/Answered online/)).toBeNull();
    expect(screen.queryByText(/Answered on this computer/)).toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("keeps today's line when an answer on this computer has no reason", async () => {
    await open(null, finished({ type: "computer", reason: null }));
    expect(await screen.findByText("Answered from 1 thing you kept")).toBeInTheDocument();
  });
});

describe("what was left out of an online question", () => {
  const NOTES: Array<[LeftOut, string]> = [
    [
      "private",
      "Some things you marked private were left out. Ask on this computer to include them.",
    ],
    [
      "sensitive",
      "Some health, money or ID details were left out. Ask on this computer to include them.",
    ],
    [
      "both",
      "Some things you marked private, and some health, money or ID details, were left out. Ask on this computer to include them.",
    ],
  ];

  it.each(NOTES)("says so for %s and offers to ask here", async (leftOut, words) => {
    const engine = await open(
      onlineViewOn(),
      finished({ type: "online", host: HOST }, { leftOut }),
    );
    expect(await screen.findByText(words)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Ask on this computer" }));
    await waitFor(() => expect(engine.asked).toHaveLength(2));
    expect(engine.asked[1]).toEqual({
      question: "When is Leo’s swimming?",
      place: "thisComputer",
    });
  });

  it("says nothing, and offers nothing, when the answer was written here and kept here", async () => {
    await open(
      onlineViewOn(),
      finished({ type: "computer", reason: "keptHere" }, { leftOut: "private" }),
    );
    await screen.findByText(/these stay on this computer/);
    expect(screen.queryByText(/were left out/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Ask on this computer" })).toBeNull();
  });

  it("keeps an online answer that landed, in Earlier, when it is asked again here", async () => {
    const engine = await open(
      onlineViewOn(),
      finished({ type: "online", host: HOST }, { leftOut: "private" }),
    );
    await screen.findByText(/Some things you marked private/);
    await userEvent.click(screen.getByRole("button", { name: "Ask on this computer" }));
    await waitFor(() => expect(engine.asked).toHaveLength(2));
    const earlier = await screen.findByRole("region", { name: "Earlier" });
    expect(within(earlier).getByText(/Answered online by your provider/)).toBeVisible();
    expect(within(earlier).getByRole("link", { name: "See what was sent" })).toHaveAttribute(
      "href",
      "#/settings/online/sent?send=7",
    );
  });

  it("moves focus to the question when the buttons go", async () => {
    await open(onlineViewOn(), finished({ type: "online", host: HOST }, { leftOut: "private" }));
    await userEvent.click(await screen.findByRole("button", { name: "Ask on this computer" }));
    await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveFocus());
  });

  it("says nothing when nothing was left out", async () => {
    await open(onlineViewOn(), finished({ type: "online", host: HOST }));
    await screen.findByText(/^Answered online by your provider/);
    expect(screen.queryByText(/were left out/)).toBeNull();
  });
});

describe("an online answer that is not sure", () => {
  it("says what came back was not shown and offers the computer, without sending again", async () => {
    const engine = await open(onlineViewOn(), async (e) => {
      const steps = await oneMemoryAnswer(e);
      const near = steps[1]?.receipts?.[0];
      return [
        { phase: "searching" },
        {
          phase: "notSure",
          answeredBy: { type: "online", host: HOST },
          sendId: 4,
          near: near ? [{ text: "Leo swims on Mondays.", receipt: near }] : [],
        },
      ];
    });
    expect(await screen.findByText("Asked online, your provider (api.example.com)")).toBeVisible();
    expect(
      screen.getByText(/didn’t match anything you kept, so Nocturne won’t show it/),
    ).toBeVisible();
    expect(engine.asked).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Ask on this computer" }));
    await waitFor(() => expect(engine.asked[1]?.place).toBe("thisComputer"));
  });
});

describe("while an online answer is on its way", () => {
  it("shows what is done and what is happening, then offers the computer when it is slow", async () => {
    const engine = await open(onlineViewOn(), () => [
      { phase: "searching" },
      { phase: "askingOnline", found: 2 },
      { phase: "waitingOnline", found: 2 },
    ]);
    expect(await screen.findByText("Asking your provider (api.example.com)…")).toBeVisible();
    expect(screen.getByText("Looked through what you’ve kept")).toBeVisible();
    expect(screen.getByText("Found 2 things you kept about this")).toBeVisible();
    expect(
      await screen.findByText("Still waiting: your provider is taking a while."),
    ).toBeVisible();
    expect(
      screen.getByText("Nocturne switches to this computer by itself after 20 seconds."),
    ).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Answer on this computer instead" }));
    await waitFor(() => expect(engine.asked).toHaveLength(2));
    expect(engine.asked[1]?.place).toBe("thisComputer");
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
  });

  it("puts the wait note in the live region when it appears", async () => {
    await open(onlineViewOn(), () => [
      { phase: "searching" },
      { phase: "askingOnline", found: 2 },
      { phase: "waitingOnline", found: 2 },
    ]);
    await screen.findByText("Still waiting: your provider is taking a while.");
    await waitFor(() => {
      const said = announcer()?.textContent ?? "";
      expect(said).toContain("your provider is taking a while");
      expect(said).toContain("Answer on this computer instead");
    });
  });

  it("says the provider was slow while it switches", async () => {
    await open(onlineViewOn(), () => [
      { phase: "searching" },
      { phase: "askingOnline", found: 2 },
      { phase: "switchingToComputer", found: 2 },
    ]);
    expect(
      await screen.findByText(
        "Nocturne stopped waiting because your provider was slow. Nothing more was sent.",
      ),
    ).toBeVisible();
    expect(screen.getByText("Switching to this computer…")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Answer on this computer instead" })).toBeNull();
  });
});

describe("the choice for one question", () => {
  it("is passed to the engine from the bar and does not carry over to the next question", async () => {
    const engine = await open(
      onlineViewOn(),
      async (e) =>
        oneMemoryAnswer(e, { phase: "done", answeredBy: { type: "online", host: HOST } }),
      null,
    );
    const bar = await screen.findByRole("textbox", { name: "Keep something, or ask" });
    const top = within(bar.closest("search") as HTMLElement);
    await userEvent.type(bar, "When is Leo’s swimming?");
    await userEvent.click(await top.findByRole("radio", { name: "This computer" }));
    await userEvent.click(top.getByRole("button", { name: /^Ask/ }));
    await waitFor(() => expect(engine.asked).toHaveLength(1));
    expect(engine.asked[0]?.place).toBe("thisComputer");
    await screen.findByText(/^Answered online by your provider/);

    await userEvent.type(bar, "And the coach?");
    expect(await top.findByRole("radio", { name: "Online · api.example.com" })).toBeChecked();
    await userEvent.click(top.getByRole("button", { name: /^Ask/ }));
    await waitFor(() => expect(engine.asked).toHaveLength(2));
    expect(engine.asked[1]?.place).toBe("auto");
  });

  it("is offered on the follow-up form before it is sent, and resets after one question", async () => {
    const engine = await open(onlineViewOn(), finished({ type: "online", host: HOST }));
    await screen.findByText(/^Answered online by your provider/);
    const follow = screen.getByRole("textbox", { name: "Ask a follow-up" });
    const group = await screen.findAllByRole("radiogroup", { name: "Answer this" });
    const where = group.at(-1) as HTMLElement;
    await userEvent.click(within(where).getByRole("radio", { name: "This computer" }));
    await userEvent.type(follow, "And the coach?{Enter}");
    await waitFor(() => expect(engine.asked).toHaveLength(2));
    expect(engine.asked[1]).toEqual({ question: "And the coach?", place: "thisComputer" });
    await act(async () => undefined);
    expect(
      within(
        screen.getAllByRole("radiogroup", { name: "Answer this" }).at(-1) as HTMLElement,
      ).getByRole("radio", { name: "Online · api.example.com" }),
    ).toBeChecked();
  });

  it("is not offered while Online answers is paused, and the computer is the only choice", async () => {
    await open(
      onlineViewOn({ state: { type: "paused", pause: { type: "keyRefused" } } }),
      finished({ type: "computer", reason: "keyRefused" }),
    );
    await screen.findByText(/your provider refused the key/);
    const where = (await screen.findAllByRole("radiogroup", { name: "Answer this" })).at(-1);
    expect(
      within(where as HTMLElement).getByRole("radio", { name: "Online · paused" }),
    ).toBeDisabled();
  });
});
