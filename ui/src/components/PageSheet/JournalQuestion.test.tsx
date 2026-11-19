import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JournalAsk, PageView, QuestionChoice } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { PageSheet } from "./PageSheet";

const NOW = new Date("2026-11-20T09:00:00");
const engineWith = () => createMockEngine({ now: NOW, journalQuestion: true });

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

async function pageWith(ask: Partial<JournalAsk>, answered: QuestionChoice | null = null) {
  const page = await engineWith().getPage();
  if (!page.question?.journal) throw new Error("The mock page has no journal question");
  return {
    ...page,
    question: {
      ...page.question,
      answered,
      journal: { ...page.question.journal, ...ask },
    },
  } satisfies PageView;
}

const paragraph = (text: RegExp) => screen.getByText(text, { selector: "p" });

describe("the journal question on the page", () => {
  it("opens with Yesterday when the block was kept on the event's day", async () => {
    const page = await pageWith({});
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "One question · from your journal" })).toBeVisible();
    expect(paragraph(/^Yesterday you wrote:/)).toHaveTextContent(
      "Yesterday you wrote: “Parents’ evening at six. Not looking forward to it.” How did it go?",
    );
    expect(screen.getByText(/Parents’ evening/, { selector: "em" })).toBeVisible();
  });

  it("opens with the weekday for a block 1 to 6 days before", async () => {
    const page = await pageWith({ writtenOn: "2026-11-16", eventDay: "2026-11-19" });
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(paragraph(/^On Monday you wrote:/)).toBeVisible();
  });

  it("opens with the date for a block 7 to 30 days before", async () => {
    const page = await pageWith({ writtenOn: "2026-11-04", eventDay: "2026-11-19" });
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(paragraph(/^On 4 November you wrote:/)).toBeVisible();
  });

  it("says said for a voice note and names it by its length", async () => {
    const page = await pageWith({ voice: true });
    const { question } = page;
    if (!question?.receipt) throw new Error("The mock question has no receipt");
    const voice: PageView = {
      ...page,
      question: {
        ...question,
        receipt: { ...question.receipt, kind: "audio", durationMs: 42_000 },
      },
    };
    render(<PageSheet page={voice} onAction={vi.fn()} />);
    expect(paragraph(/^Yesterday you said:/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Voice note · 0:42. Source." })).toBeVisible();
  });

  it("shows the receipt chip to the block and opens it", async () => {
    const page = await pageWith({});
    const onAction = vi.fn();
    render(<PageSheet page={page} onAction={onAction} />);
    const chip = screen.getByRole("button", { name: /^Entry · .+ · \d{1,2}:\d{2} [ap]m/ });
    await userEvent.click(chip);
    expect(onAction).toHaveBeenCalledWith({
      type: "open",
      receipt: expect.objectContaining({ itemId: "j-block-question" }),
    });
  });

  it("offers Fine, Not great and Not yet", async () => {
    const page = await pageWith({});
    const onAction = vi.fn();
    render(<PageSheet page={page} onAction={onAction} />);
    expect(screen.getAllByRole("button", { name: /^(Fine|Not great|Not yet)$/ })).toHaveLength(3);
    await userEvent.click(screen.getByRole("button", { name: "Not great" }));
    expect(onAction).toHaveBeenCalledWith({ type: "answer", choice: "notGreat" });
  });

  it("keeps the answer and asks for an optional line", async () => {
    const page = await pageWith({}, "notGreat");
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(screen.getByText("Not great. Kept on today’s journal.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Fine" })).toBeNull();
    expect(screen.getByLabelText("Anything to add? (optional)")).toHaveAttribute(
      "placeholder",
      "A line, if you like.",
    );
  });

  it("sends the line with Add", async () => {
    const page = await pageWith({}, "fine");
    const onAction = vi.fn();
    render(<PageSheet page={page} onAction={onAction} />);
    expect(screen.getByRole("button", { name: "Add" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Anything to add? (optional)"), "Went well{enter}");
    expect(onAction).toHaveBeenCalledWith({ type: "addLine", line: "Went well" });
  });

  it("shows the added line and the way to today's journal", async () => {
    const page = await pageWith({ line: "Mrs Okafor is worried about his reading." }, "notGreat");
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(screen.getByText("“Mrs Okafor is worried about his reading.” Added.")).toBeVisible();
    expect(screen.getByRole("link", { name: "Open today’s journal ›" })).toHaveAttribute(
      "href",
      "#/journal",
    );
    expect(screen.queryByLabelText("Anything to add? (optional)")).toBeNull();
  });
});

describe("answering on Today", () => {
  it("keeps the answer at once, adds the line, and puts both on today's journal", async () => {
    const engine = engineWith();
    const answer = vi.spyOn(engine, "answerQuestion");
    const addLine = vi.spyOn(engine, "addAnswerLine");
    renderApp({ hash: "#/today", engine });
    await userEvent.click(await screen.findByRole("button", { name: "Not great" }));
    expect(answer).toHaveBeenCalledWith(expect.any(String), "notGreat");
    expect(await screen.findByText("Not great. Kept on today’s journal.")).toBeVisible();
    await userEvent.type(screen.getByLabelText("Anything to add? (optional)"), "Reading worry");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(addLine).toHaveBeenCalledWith(expect.any(String), "Reading worry");
    expect(await screen.findByText("“Reading worry” Added.")).toBeVisible();

    const day = await engine.getJournalDay();
    const row = day.rows.find((r) => r.kind === "answer");
    expect(row?.answer).toMatchObject({
      choice: "notGreat",
      line: "Reading worry",
      sentence: "Parents’ evening at six. Not looking forward to it.",
    });
  });
});

describe("the page headline", () => {
  it("counts what the rail counts, the name question included", async () => {
    const engine = engineWith();
    renderApp({ hash: "#/today", engine });
    const rail = await screen.findByRole("navigation", { name: "Main" });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Three things need you today." }),
    ).toBeVisible();
    expect(within(rail).getByRole("link", { name: /Needs you/ })).toHaveTextContent("3");
    expect(screen.getByRole("link", { name: "1 more this week in Needs you" })).toBeVisible();
  });
});
