import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OnlineSend, SentLine } from "../../engine/bindings";
import { onlineViewOn } from "../../engine/mock/onlineAsk";
import { renderApp, setViewport } from "../../test/render";
import { onlineEngine } from "../ask/testing";

// Midday, so "an hour ago" is still today whenever the test runs.
const now = new Date().setHours(12, 0, 0, 0);
const today = (hours: number) => new Date(now - hours * 3_600_000).toISOString();
const OLD = "2020-03-05T21:40:00";

const facts = (...memory: SentLine[]): SentLine[] => [
  { type: "text", text: "Facts:" },
  ...memory,
  { type: "text", text: "Question: when is my eye test?" },
];

const send = (over: Partial<OnlineSend> & Pick<OnlineSend, "id">): OnlineSend => ({
  at: today(1),
  kind: "question",
  host: "api.example.com",
  model: "llama-3.3-70b-instruct",
  question: "When is my eye test?",
  lines: facts(
    { type: "memory", n: 1, text: "The eye test is on Friday at 11:20.", state: "same" },
    { type: "memory", n: 2, text: "Results come by 27 November.", state: "same" },
  ),
  instructions: "Answer only from the facts.",
  bytesUp: 1400,
  bytesDown: 600,
  costUsd: null,
  result: { type: "answered", receipts: 2 },
  ...over,
});

async function open(sends: OnlineSend[], hash = "#/settings/online/sent") {
  setViewport(1280);
  const engine = onlineEngine(onlineViewOn());
  vi.spyOn(engine.online, "sends").mockImplementation(async (limit, before) =>
    sends.filter((s) => before === null || s.id < before).slice(0, limit),
  );
  renderApp({ hash, engine });
  return engine;
}

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

describe("what was sent", () => {
  it("sent.empty: says nothing has been sent and what will appear", async () => {
    await open([]);
    expect(await screen.findByText("Nothing has been sent online.")).toBeVisible();
    expect(screen.getByRole("heading", { level: 1, name: "Sent online" })).toBeVisible();
    expect(screen.queryByRole("navigation", { name: "Sent by day" })).toBeNull();
  });

  it("sent.groups_by_day: lists entries under their day, newest first", async () => {
    await open([
      send({ id: 3, at: today(1) }),
      send({ id: 2, at: today(2), question: "What did Sam say?" }),
      send({ id: 1, at: OLD, question: "When does the permit run out?" }),
    ]);
    const nav = await screen.findByRole("navigation", { name: "Sent by day" });
    const headings = within(nav)
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent);
    expect(headings[0]).toBe("Today");
    expect(headings[1]).toBe("Thursday 5 March");
    expect(within(nav).getAllByRole("button")).toHaveLength(3);
    expect(within(nav).getAllByText(/1\.4 kB · answered/)).toHaveLength(3);
    expect(
      within(nav).getByText(
        "38 questions this month. Older than 90 days are removed by themselves.",
      ),
    ).toBeVisible();
  });

  it("shows the exact words that were sent and where they went", async () => {
    await open([send({ id: 1 })]);
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(
      within(entry).getByText("By Nocturne’s engine, to your provider (api.example.com)"),
    ).toBeVisible();
    expect(within(entry).getByText("1.4 kB sent, 600 B back")).toBeVisible();
    expect(within(entry).getByText("Not told by your provider")).toBeVisible();
    expect(
      within(entry).getByText("Answered, with 2 receipts checked on this computer"),
    ).toBeVisible();
    const text = within(entry).getByText(/^\[m1\]/).parentElement as HTMLElement;
    expect(text.textContent).toBe(
      "Facts:[m1] The eye test is on Friday at 11:20.[m2] Results come by 27 November.Question: when is my eye test?",
    );
  });

  it("shows a cost only when the provider told it", async () => {
    await open([send({ id: 1, costUsd: 0.0012 })]);
    expect(await screen.findByText("US$0.0012")).toBeVisible();
    expect(screen.queryByText("Not told by your provider")).toBeNull();
  });

  it("sent.deleted_memory_line: says a deleted memory is gone and never shows its words", async () => {
    await open([
      send({
        id: 1,
        lines: facts(
          { type: "memory", n: 1, text: "The permit runs out on 31 January.", state: "same" },
          { type: "memory", n: 2, text: null, state: "deleted" },
        ),
      }),
    ]);
    expect(
      await screen.findByText("A memory you have since deleted. Nocturne no longer has its words."),
    ).toBeVisible();
    expect(
      screen.getByText(
        "The memory’s words were sent to your provider at the time. Nocturne can’t take them back from it.",
      ),
    ).toBeVisible();
  });

  it("says a changed memory has changed, without showing the old words", async () => {
    await open([
      send({
        id: 1,
        lines: facts({ type: "memory", n: 1, text: null, state: "changed" }),
      }),
    ]);
    expect(
      await screen.findByText(
        "A memory you have since changed. Nocturne no longer shows its old words.",
      ),
    ).toBeVisible();
  });

  it("sent.failed_entry_says_nothing_left: a question that did not go says nothing left", async () => {
    await open([
      send({
        id: 1,
        question: "What did Sam say about Saturday?",
        lines: [],
        instructions: "",
        bytesUp: 0,
        bytesDown: 0,
        result: { type: "notSent", reason: "noInternet" },
      }),
    ]);
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(
      within(entry).getByText("Not sent: no internet. Answered on this computer."),
    ).toBeVisible();
    expect(within(entry).getByText("Nothing left this computer")).toBeVisible();
    expect(within(entry).queryByText("Exactly what was sent")).toBeNull();
    expect(within(entry).queryByText("Cost")).toBeNull();
    expect(within(entry).getByText(/lists questions that didn’t go too/)).toBeVisible();
    expect(screen.getByText(/not sent, no internet/)).toBeVisible();
  });

  it("says nothing left when a try failed before a byte went", async () => {
    await open([
      send({
        id: 1,
        lines: facts({ type: "memory", n: 1, text: "Secret words.", state: "same" }),
        bytesUp: 0,
        bytesDown: 0,
        result: { type: "failed", reason: "couldntReach" },
      }),
    ]);
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(within(entry).queryByText("Sent")).toBeNull();
    expect(
      within(entry).getByText("Not sent: couldn’t reach your provider. Answered on this computer."),
    ).toBeVisible();
    expect(within(entry).getByText("Nothing left this computer")).toBeVisible();
    expect(within(entry).queryByText("Exactly what was sent")).toBeNull();
    expect(within(entry).queryByText(/Secret words/)).toBeNull();
  });

  it("keeps saying sent when a try failed after bytes went", async () => {
    await open([send({ id: 1, result: { type: "failed", reason: "slow" } })]);
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(within(entry).getByText("Sent")).toBeVisible();
    expect(within(entry).getByText("Exactly what was sent")).toBeVisible();
  });

  it("finds a linked entry on a later page, and never opens another when it is gone", async () => {
    const many = Array.from({ length: 120 }, (_, i) =>
      send({ id: 200 - i, at: today(1 + i / 100), question: `Question ${200 - i}` }),
    );
    await open(many, "#/settings/online/sent?send=85");
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(within(entry).getByRole("heading", { level: 2, name: "Question 85" })).toBeVisible();
  });

  it("says a linked entry is gone, without opening a different one", async () => {
    await open([send({ id: 5, question: "Newest?" })], "#/settings/online/sent?send=2");
    expect(await screen.findByText(/That entry is no longer in the list/)).toBeVisible();
    expect(screen.queryByRole("region", { name: "Entry" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 2, name: "Newest?" })).toBeNull();
  });

  it("names a connection check as one, and says it worked", async () => {
    await open([
      send({ id: 1, kind: "check", question: "Reply with the word ready.", bytesUp: 200 }),
    ]);
    const nav = await screen.findByRole("navigation", { name: "Sent by day" });
    expect(within(nav).getByText("Connection check")).toBeVisible();
    expect(within(nav).getByText(/worked/)).toBeVisible();
  });

  it("shows the standard instructions only when asked", async () => {
    await open([send({ id: 1 })]);
    const toggle = await screen.findByRole("button", {
      name: "Show Nocturne’s standard instructions, the same for every question",
    });
    expect(screen.queryByText("Answer only from the facts.")).toBeNull();
    await userEvent.click(toggle);
    expect(screen.getByText("Answer only from the facts.")).toBeVisible();
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("opens the entry named in the address, and another on a click", async () => {
    await open(
      [send({ id: 5, question: "Newest?" }), send({ id: 4, question: "The one linked" })],
      "#/settings/online/sent?send=4",
    );
    const entry = await screen.findByRole("region", { name: "Entry" });
    expect(within(entry).getByRole("heading", { level: 2, name: "The one linked" })).toBeVisible();
    const nav = screen.getByRole("navigation", { name: "Sent by day" });
    expect(within(nav).getByRole("button", { name: /The one linked/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await userEvent.click(within(nav).getByRole("button", { name: /Newest\?/ }));
    expect(
      within(screen.getByRole("region", { name: "Entry" })).getByRole("heading", {
        level: 2,
        name: "Newest?",
      }),
    ).toBeVisible();
  });

  it("loads earlier entries on request", async () => {
    const many = Array.from({ length: 60 }, (_, i) =>
      send({ id: 100 - i, at: today(1 + i / 100), question: `Question ${100 - i}` }),
    );
    await open(many);
    const nav = await screen.findByRole("navigation", { name: "Sent by day" });
    expect(within(nav).getAllByRole("button", { name: /Question/ })).toHaveLength(50);
    await userEvent.click(within(nav).getByRole("button", { name: "Show earlier" }));
    await waitFor(() =>
      expect(within(nav).getAllByRole("button", { name: /Question/ })).toHaveLength(60),
    );
    expect(within(nav).queryByRole("button", { name: "Show earlier" })).toBeNull();
  });

  it("says so, with a way to try again, when the list cannot be read", async () => {
    const engine = onlineEngine(onlineViewOn());
    vi.spyOn(engine.online, "sends").mockRejectedValue(new Error("no"));
    renderApp({ hash: "#/settings/online/sent", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn’t read this list");
    expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
  });
});
