import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MemoryView, NeedsYouEntry } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

/** The mock lists a which-is-right and a due form; this one adds what the mock lacks. */
class Waiting extends MockEngine {
  extra: Array<(memories: MemoryView[]) => NeedsYouEntry> = [];
  failList = false;

  override async listNeedsYou(): Promise<NeedsYouEntry[]> {
    if (this.failList) throw new Error("no");
    const base = await super.listNeedsYou();
    const memories = await this.listMemories({
      entityId: null,
      itemId: null,
      status: null,
      query: null,
      limit: null,
    });
    return [...base, ...this.extra.map((make) => make(memories))];
  }
}

const unconfirmed =
  (agreeing = 0, used = false) =>
  (memories: MemoryView[]) =>
    ({
      type: "unconfirmed",
      memory: memories.find((m) => m.id === "mem-nut"),
      agreeingSources: agreeing,
      used,
    }) as NeedsYouEntry;

const promise = (memories: MemoryView[]) =>
  ({
    type: "promise",
    memory: memories.find((m) => m.id === "mem-lake"),
    done: false,
  }) as NeedsYouEntry;

beforeEach(() => setViewport(1280));
afterEach(() => {
  vi.useRealTimers();
  window.location.hash = "";
});

/** Lets the "Marked as done" toast run out, which is when the press is committed. */
const toastEnds = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(10_500);
  });

const open = async (engine = new Waiting()) => {
  renderApp({ hash: "#/needs-you", engine });
  await screen.findByRole("heading", { level: 1 });
  await screen.findByRole("list", { name: "Things waiting for you" });
  return engine;
};

const rows = () =>
  within(screen.getByRole("list", { name: "Things waiting for you" })).getAllByRole("listitem");
const said = () =>
  screen
    .getAllByRole("status")
    .find(
      (s) => s.getAttribute("aria-live") === "polite" && s.className.includes("said"),
    ) as HTMLElement;

describe("what is waiting", () => {
  it("counts them, names them and shows the keys", async () => {
    await open();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("2 things are waiting");
    expect(screen.getByText("Needs you · 2")).toBeInTheDocument();
    expect(screen.getByText("J / K to move · 1 2 3 to choose")).toBeInTheDocument();
    expect(rows()).toHaveLength(2);
  });

  it("a which-is-right shows both sources with where each came from and a way to choose", async () => {
    await open();
    expect(
      screen.getByRole("heading", { level: 2, name: "Your dentist appointment" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Which is right?")).toBeInTheDocument();
    const group = screen.getByRole("group", { name: "Choose the right one" });
    expect(within(group).getAllByRole("button", { name: /^This one:/ })).toHaveLength(2);
    expect(within(group).getByText(/^Appointment card · /)).toBeInTheDocument();
    expect(within(group).getByText(/^Voice note · 0:08 · /)).toBeInTheDocument();
  });

  it("a due form shows its badge, its source and Done", async () => {
    await open();
    expect(screen.getByText(/^Due (Friday|Fri )/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^Mark as done: The Year 4 trip form/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Letter · / })).toBeInTheDocument();
  });

  it("an unconfirmed one says what it is, where from and that nothing else says it", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed()];
    await open(engine);
    expect(screen.getByText("Unconfirmed")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "“Leo is allergic to nuts”" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Nothing else you kept says this. It has not been used anywhere."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Web page you pasted/ })).toBeInTheDocument();
    for (const name of ["It’s true", "Not true, delete it…", "Leave unconfirmed"]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("other sources that agree are counted", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed(2, true)];
    await open(engine);
    expect(
      screen.getByText("2 other things you kept say this. It has been used in an answer."),
    ).toBeInTheDocument();
  });

  it("a promise has Done like a due form", async () => {
    const engine = new Waiting();
    engine.extra = [promise];
    await open(engine);
    expect(screen.getByText("A promise")).toBeInTheDocument();
    const spy = vi.spyOn(engine, "markDone");
    vi.useFakeTimers({ shouldAdvanceTime: true });
    await userEvent.click(
      screen.getByRole("button", { name: /^Mark as done: You decided to book/ }),
    );
    await toastEnds();
    expect(spy).toHaveBeenCalledWith("mem-lake");
  });

  it("Undo puts the entry back and never marks it", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const engine = await open();
    const spy = vi.spyOn(engine, "markDone");
    await userEvent.click(screen.getByRole("button", { name: /^Mark as done:/ }));
    expect(screen.queryByRole("button", { name: /^Mark as done:/ })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.getByRole("button", { name: /^Mark as done:/ })).toBeInTheDocument();
    await toastEnds();
    expect(spy).not.toHaveBeenCalled();
  });

  it("marks a press that is still waiting when the screen goes away", async () => {
    const engine = new Waiting();
    const view = renderApp({ hash: "#/needs-you", engine });
    await screen.findByRole("list", { name: "Things waiting for you" });
    const spy = vi.spyOn(engine, "markDone");
    await userEvent.click(screen.getByRole("button", { name: /^Mark as done:/ }));
    expect(spy).not.toHaveBeenCalled();
    view.unmount();
    expect(spy).toHaveBeenCalledWith("mem-trip");
  });
});

describe("acting on what is waiting", () => {
  it("choosing the right dentist time resolves it, says so, and the count drops", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "resolveWhich");
    const group = screen.getByRole("group", { name: "Choose the right one" });
    await userEvent.click(
      within(group).getAllByRole("button", { name: /^This one:/ })[1] as HTMLElement,
    );
    expect(spy).toHaveBeenCalledWith("conflict-dentist", "mem-dentist-thu");
    await waitFor(() =>
      expect(said()).toHaveTextContent(
        /^Dentist set to Thursday .*The other is kept as history\.$/,
      ),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("One thing is waiting");
    expect(
      screen.queryByRole("heading", { level: 2, name: "Your dentist appointment" }),
    ).toBeNull();
  });

  it("Done on the trip form says so and, with nothing left, shows the calm state", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "markDone");
    const group = screen.getByRole("group", { name: "Choose the right one" });
    await userEvent.click(
      within(group).getAllByRole("button", { name: /^This one:/ })[0] as HTMLElement,
    );
    vi.useFakeTimers({ shouldAdvanceTime: true });
    await userEvent.click(await screen.findByRole("button", { name: /^Mark as done:/ }));
    expect(spy).not.toHaveBeenCalled();
    await toastEnds();
    expect(spy).toHaveBeenCalledWith("mem-trip");
    expect(await screen.findByText("Nothing needs you.")).toBeInTheDocument();
    expect(screen.getByText("Anything new will be on tomorrow’s page.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("All clear");
    expect(said()).toHaveTextContent("Year 4 trip form done.");
    expect(screen.getByText("Nothing needs you.").closest("[role=status]")).toHaveFocus();
  });

  it("It’s true confirms and says so", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed()];
    await open(engine);
    const spy = vi.spyOn(engine, "confirmMemory");
    await userEvent.click(screen.getByRole("button", { name: "It’s true" }));
    expect(spy).toHaveBeenCalledWith("mem-nut", true);
    await waitFor(() =>
      expect(said()).toHaveTextContent(
        "Confirmed: Leo is allergic to nuts. Marked as sure because you said so.",
      ),
    );
  });

  it("Leave unconfirmed snoozes it for 7 days and moves focus on", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed()];
    await open(engine);
    const spy = vi.spyOn(engine, "snooze");
    await userEvent.click(screen.getByRole("button", { name: "Leave unconfirmed" }));
    expect(spy).toHaveBeenCalledWith("mem-nut", 7);
    await waitFor(() =>
      expect(said()).toHaveTextContent("Left unconfirmed. It won’t be used in answers."),
    );
    // The entry after it takes the focus.
    expect(rows()).toHaveLength(2);
    expect(rows()[1]).toHaveFocus();
    expect(rows()[1]).toHaveAttribute("data-entry", "due:mem-trip");
  });

  it("Not true, forget it opens the shared forget dialog for that memory", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed()];
    await open(engine);
    await userEvent.click(screen.getByRole("button", { name: "Not true, delete it…" }));
    expect(await screen.findByRole("dialog", { name: "Forget 1 memory?" })).toBeVisible();
  });

  it("an action that fails says so and keeps the entry", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const engine = await open();
    vi.spyOn(engine, "markDone").mockRejectedValue(new Error("no"));
    await userEvent.click(screen.getByRole("button", { name: /^Mark as done:/ }));
    await toastEnds();
    await waitFor(() =>
      expect(said()).toHaveTextContent("That didn’t go through, so nothing was changed."),
    );
    expect(rows()).toHaveLength(2);
  });

  it("focus moves to the next entry after an action", async () => {
    await open();
    const group = screen.getByRole("group", { name: "Choose the right one" });
    await userEvent.click(
      within(group).getAllByRole("button", { name: /^This one:/ })[0] as HTMLElement,
    );
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(rows()[0]).toHaveFocus();
  });
});

describe("keyboard", () => {
  it("J and K move between entries while focus is in the list", async () => {
    await open();
    const [first, second] = rows() as [HTMLElement, HTMLElement];
    act(() => first.focus());
    await userEvent.keyboard("j");
    expect(second).toHaveFocus();
    await userEvent.keyboard("j");
    expect(second).toHaveFocus();
    await userEvent.keyboard("k");
    expect(first).toHaveFocus();
  });

  it("1 2 3 choose in the entry that has focus", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "resolveWhich");
    const [first] = rows() as [HTMLElement];
    act(() => first.focus());
    await userEvent.keyboard("2");
    expect(spy).toHaveBeenCalledWith("conflict-dentist", "mem-dentist-thu");
  });

  it("3 leaves an unconfirmed one unconfirmed", async () => {
    const engine = new Waiting();
    engine.extra = [unconfirmed()];
    await open(engine);
    const spy = vi.spyOn(engine, "snooze");
    act(() => rows()[1]?.focus());
    await userEvent.keyboard("3");
    expect(spy).toHaveBeenCalledWith("mem-nut", 7);
  });

  it("does nothing from outside the list", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "resolveWhich");
    act(() => screen.getByRole("main").focus());
    await userEvent.keyboard("1j2");
    expect(spy).not.toHaveBeenCalled();
  });

  it("ignores a digit with no choice behind it", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "markDone");
    act(() => rows()[1]?.focus());
    await userEvent.keyboard("3");
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("a receipt", () => {
  it("opens the original", async () => {
    await open();
    await userEvent.click(screen.getAllByText(/^Appointment card · /)[0] as HTMLElement);
    expect(window.location.hash).toBe("#/items/item-card");
  });
});

describe("when the list cannot be read", () => {
  it("says so in words and offers to try again", async () => {
    const engine = new Waiting();
    engine.failList = true;
    renderApp({ hash: "#/needs-you", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t get this list. What you kept is safe.",
    );
    engine.failList = false;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "2 things are waiting",
    );
  });
});
