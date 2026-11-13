import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import type { PageView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { PageSheet } from "./PageSheet";

const engine = createMockEngine();
let today: PageView;
const byDate: Record<string, PageView> = {};

beforeAll(async () => {
  today = await engine.getPage();
  for (const [name, date] of Object.entries({
    first: "2026-10-31",
    heavy: "2026-11-04",
    quiet: "2026-11-05",
    writing: "2026-11-06",
    catchUp: "2026-11-07",
  })) {
    byDate[name] = await engine.getPage(date);
  }
});

const heading = (name: string | RegExp, level = 2) => screen.getByRole("heading", { level, name });

describe("PageSheet, a normal day", () => {
  it("leads with the headline and says what the night did", () => {
    render(<PageSheet page={today} onAction={vi.fn()} />);
    expect(heading("Two things need you today.", 1)).toBeInTheDocument();
    expect(
      screen.getByText(
        "Overnight Nocturne read 14 things you kept, noticed 3 changes and let 3 go.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Friday 13 November")).toBeInTheDocument();
  });

  it("is an article with its sections, Needs you first and the only one in Ember", () => {
    render(<PageSheet page={today} onAction={vi.fn()} />);
    expect(
      screen.getByRole("article", { name: "Morning page, Friday 13 November" }),
    ).toBeInTheDocument();
    const titles = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(titles).toEqual(["Needs you · 2", "What changed · 3", "Let go · 3", "One question"]);
    expect(heading("Needs you · 2")).toHaveAttribute("data-tone", "ember");
    expect(heading("What changed · 3")).not.toHaveAttribute("data-tone");
  });

  it("gives every claim a receipt", () => {
    render(<PageSheet page={today} onAction={vi.fn()} />);
    const changed = within(screen.getByRole("region", { name: "What changed · 3" }));
    expect(changed.getAllByRole("button", { name: /^Source:/ })).toHaveLength(3);
    const needs = within(screen.getByRole("region", { name: "Needs you · 2" }));
    expect(needs.getAllByRole("button", { name: /^Source:/ }).length).toBeGreaterThanOrEqual(1);
    expect(needs.getByText(/Appointment card/)).toBeInTheDocument();
  });

  it("opens the original when a receipt is pressed", async () => {
    const onAction = vi.fn();
    render(<PageSheet page={today} onAction={onAction} />);
    await userEvent.click(screen.getAllByRole("button", { name: /^Source:/ })[0] as HTMLElement);
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "open" }));
  });

  it("bolds only what the engine marked", () => {
    render(<PageSheet page={today} onAction={vi.fn()} />);
    expect(screen.getByText("next Friday").tagName).toBe("STRONG");
    expect(screen.getByText("Monday").tagName).toBe("STRONG");
    expect(screen.getByText("4471").tagName).toBe("STRONG");
    expect(document.body.textContent).not.toContain("**");
  });

  it("sends a which-is-right choice and announces the result politely", async () => {
    const onAction = vi.fn();
    const { rerender } = render(<PageSheet page={today} onAction={onAction} />);
    const which = today.needsYou.find((l) => l.kind === "which");
    const thursday = which?.choices[1];
    await userEvent.click(screen.getByRole("button", { name: new RegExp(thursday?.text ?? "") }));
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "which",
        choice: expect.objectContaining({ memoryId: "mem-dentist-thu" }),
      }),
    );
    const done = {
      ...today,
      needsYou: today.needsYou.map((l) =>
        l.kind === "which" ? { ...l, done: true, memoryIds: ["mem-dentist-thu"] } : l,
      ),
    };
    rerender(<PageSheet page={done} onAction={onAction} />);
    const status = screen
      .getAllByRole("status")
      .find((s) => s.textContent?.includes("The other is kept as history, not deleted."));
    expect(status).toHaveTextContent(
      `Set to ${thursday?.text}. The other is kept as history, not deleted.`,
    );
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(screen.getByRole("heading", { level: 2, name: "Needs you · 1" })).toBeInTheDocument();
  });

  it("strikes a line marked as done and says so", async () => {
    const onAction = vi.fn();
    const { rerender } = render(<PageSheet page={today} onAction={onAction} />);
    await userEvent.click(screen.getByRole("button", { name: "Mark as done" }));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "done" }));
    const done = {
      ...today,
      needsYou: today.needsYou.map((l) => (l.kind === "due" ? { ...l, done: true } : l)),
    };
    rerender(<PageSheet page={done} onAction={onAction} />);
    expect(screen.queryByRole("button", { name: "Mark as done" })).toBeNull();
    expect(screen.getByText(/The Year 4 trip form is due/).closest("p")).toHaveAttribute(
      "data-done",
      "true",
    );
    expect(screen.getByText("Done. Nocturne will stop reminding you.")).toBeInTheDocument();
  });

  it("brings back a let-go line, but never a merged one", async () => {
    const onAction = vi.fn();
    const { rerender } = render(<PageSheet page={today} onAction={onAction} />);
    const buttons = screen.getAllByRole("button", { name: "Bring back" });
    expect(buttons).toHaveLength(2);
    expect(screen.getByText("Two notes about bin day were merged into one")).toBeInTheDocument();
    expect(screen.getByText("Same thing, kept twice")).toBeInTheDocument();
    expect(screen.getByText("Not used since August")).toBeInTheDocument();
    await userEvent.click(buttons[0] as HTMLElement);
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "bringBack" }));
    const restored = {
      ...today,
      letGo: today.letGo.map((l) => (l.id === "line-gym" ? { ...l, done: true } : l)),
    };
    rerender(<PageSheet page={restored} onAction={onAction} />);
    expect(screen.getByText("Brought back. It will not fade for 30 days.")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Bring back" })).toHaveLength(1);
  });

  it("asks one question, says thanks, and lets the answer change", async () => {
    const onAction = vi.fn();
    const { rerender } = render(<PageSheet page={today} onAction={onAction} />);
    expect(screen.getByText("Is Leo still doing swimming with Coach Sam?")).toBeInTheDocument();
    expect(screen.getByText(/last asked never/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Yes" }));
    expect(onAction).toHaveBeenCalledWith({ type: "answer", choice: "yes" });
    const answered = {
      ...today,
      question: today.question && { ...today.question, answered: "yes" as const },
    };
    rerender(<PageSheet page={answered} onAction={onAction} />);
    expect(screen.getByText("Kept as sure.")).toBeInTheDocument();
    expect(screen.getByText("Thanks. Nocturne will ask again in a while.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Yes" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Change answer" }));
    expect(screen.getByRole("button", { name: "Yes" })).toBeInTheDocument();
  });

  it("offers the Read, Listen and Print editions as tabs", async () => {
    const onAction = vi.fn();
    render(<PageSheet page={today} onAction={onAction} />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Read", "Listen", "Print"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("Read");
    tabs[0]?.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(tabs[1]).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onAction).toHaveBeenCalledWith({ type: "edition", edition: "audio" });
  });

  it("links to the day before and shows the page number and the footer", () => {
    render(<PageSheet page={today} onAction={vi.fn()} previousDate="2026-11-12" />);
    expect(screen.getByText(/Page 23/)).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Previous page, Thursday 12 November" }),
    ).toHaveAttribute("href", "#/pages/2026-11-12");
    expect(screen.getByText("Tidied 02:10–02:16")).toBeInTheDocument();
    expect(screen.getByText("Sent to the internet: nothing")).toBeInTheDocument();
  });

  it("redacts a forgotten line", () => {
    const page = {
      ...today,
      changed: today.changed.map((l) =>
        l.id === "line-gran" ? { ...l, forgotten: true, text: "", receipts: [] } : l,
      ),
    };
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(screen.getByText("Forgotten. This line is gone from the page.")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("4471");
  });
});

describe("PageSheet, other days", () => {
  it("first page: says so, shows what Nocturne now knows and asks about the time", () => {
    const page = byDate.first as PageView;
    render(<PageSheet page={page} onAction={vi.fn()} pageTime="07:00" />);
    expect(heading("Your first page.", 1)).toBeInTheDocument();
    expect(screen.getByText("Saturday 31 October · Page 1")).toBeInTheDocument();
    expect(heading("What Nocturne now knows · 3")).toBeInTheDocument();
    expect(heading("Tomorrow’s page gets better with")).toBeInTheDocument();
    expect(screen.getByText("Is this page arriving at a good time?")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "07:00 is right" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Earlier" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Later" })).toBeInTheDocument();
  });

  it("quiet day: shows what is coming up this week, with a receipt each", () => {
    const page = byDate.quiet as PageView;
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(heading("A quiet night. Nothing needs you.", 1)).toBeInTheDocument();
    const week = within(screen.getByRole("region", { name: "This week" }));
    expect(week.getAllByRole("button", { name: /^Source:/ })).toHaveLength(3);
    expect(week.getByText("MON", { exact: false, ignore: "x" }) ?? null).toBeTruthy();
    expect(screen.queryByRole("heading", { level: 2, name: /Needs you/ })).toBeNull();
    expect(screen.getByText("Is Gran still at Rowan Court?")).toBeInTheDocument();
  });

  it("heavy day: caps the lists and folds the rest into links", () => {
    const page = byDate.heavy as PageView;
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(heading("Seven things need you. Three are today.", 1)).toBeInTheDocument();
    expect(heading("Needs you · today")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "4 more this week in Needs you" })).toHaveAttribute(
      "href",
      "#/needs-you",
    );
    expect(heading("What changed · top 2 of 18")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "16 smaller changes" })).toBeInTheDocument();
    const lines = screen.getAllByText(/\S/, { selector: "p" }).length;
    expect(lines).toBeLessThanOrEqual(12);
  });

  it("being written: shows a progress line in words", () => {
    const page = byDate.writing as PageView;
    const { rerender } = render(<PageSheet page={page} onAction={vi.fn()} />);
    const bar = screen.getByRole("progressbar", { name: "Still writing today’s page" });
    expect(bar).toHaveAttribute(
      "aria-valuetext",
      "Still writing today’s page, 6 things still to read",
    );
    rerender(
      <PageSheet
        page={page}
        onAction={vi.fn()}
        progress={{ done: 8, total: 14, etaSeconds: 180 }}
      />,
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "8");
    expect(screen.getByText("8 of 14 read · about 3 min")).toBeInTheDocument();
  });

  it("catch-up: still reads as one page", () => {
    const page = byDate.catchUp as PageView;
    render(<PageSheet page={page} onAction={vi.fn()} />);
    expect(heading("Two nights in one page.", 1)).toBeInTheDocument();
  });
});
