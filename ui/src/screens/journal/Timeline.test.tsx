import { screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

const NOW = new Date("2026-11-20T09:00:00");

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const timeline = () =>
  screen.getByRole("heading", { level: 2, name: "Through the day" }).parentElement as HTMLElement;

describe("an answer on the timeline", () => {
  it("shows the choice, the line and what it was about, never the template", async () => {
    const engine = createMockEngine({
      now: NOW,
      journalQuestion: true,
      journal: { mode: "day", delayMs: 0 },
    });
    const date = (await engine.getPage()).date;
    await engine.answerQuestion(date, "notGreat");
    await engine.addAnswerLine(date, "Mrs Okafor is worried about his reading.");
    renderApp({ hash: "#/journal", engine });
    await screen.findByRole("heading", { level: 2, name: "Through the day" });
    const row = within(timeline()).getByText("Not great").closest("li") as HTMLElement;
    expect(row).toHaveTextContent("Mrs Okafor is worried about his reading.");
    const about = within(row).getByRole("link", {
      name: "About: “Parents’ evening at six. Not looking forward to it.”",
      hidden: true,
    });
    expect(about).toHaveAttribute("href", "#/items/j-block-question");
    expect(within(timeline()).queryByText(/How did it go/)).toBeNull();
  });

  it("is an answer row with only the choice when no line was added", async () => {
    const engine = createMockEngine({
      now: NOW,
      journalQuestion: true,
      journal: { mode: "day", delayMs: 0 },
    });
    await engine.answerQuestion((await engine.getPage()).date, "fine");
    const day = await engine.getJournalDay();
    expect(day.rows.find((r) => r.kind === "answer")?.answer?.line).toBeNull();
  });
});

describe("an empty day", () => {
  it("says what was kept on a past day, and keeps today's own words", async () => {
    const engine = createMockEngine({ now: NOW, journal: { mode: "empty", delayMs: 0 } });
    renderApp({ hash: "#/journal/2026-11-17", engine });
    expect(await screen.findByText("Nothing was kept on this day.")).toBeInTheDocument();
    window.location.hash = "#/journal";
    await waitFor(() => expect(screen.getByText(/^Nothing kept today yet\./)).toBeInTheDocument());
  });
});
