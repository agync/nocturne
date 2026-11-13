import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemFilter, ItemView } from "../../engine/bindings";
import { MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { actionFor, subline, thumbCode } from "./describe";

/** The mock for everything, with items a test can add to or take from the list. */
class Kept extends MockEngine {
  extra: ItemView[] = [];
  only: ItemView[] | null = null;
  fail = false;

  /** Keeps a note the way the real engine does: the list hears about it. */
  async keep(text: string) {
    const kept = await this.keepText(text, "quickCapture");
    (this as unknown as { emit(name: string, item: ItemView): void }).emit(
      "item-changed",
      kept.item,
    );
  }

  override async listItems(filter: ItemFilter): Promise<ItemView[]> {
    if (this.fail) throw new Error("no");
    if (this.only) return this.only;
    return [...(await super.listItems(filter)), ...this.extra];
  }
}

const base = async (
  engine: MockEngine,
  id: string,
  patch: Partial<ItemView>,
): Promise<ItemView> => ({
  ...(await engine.getItem("item-counciltax")),
  id,
  title: id,
  ...patch,
});

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const open = async (engine = new Kept()) => {
  renderApp({ hash: "#/recent", engine });
  await screen.findByRole("heading", { level: 1, name: "Today and yesterday" });
  await screen.findByRole("list", { name: "Things you kept today and yesterday" });
  return engine;
};

const rowOf = (title: string) =>
  within(screen.getByRole("list", { name: "Things you kept today and yesterday" }))
    .getByText(title)
    .closest("li") as HTMLElement;
const announcer = () =>
  screen
    .getAllByRole("status")
    .find((s) => s.getAttribute("aria-atomic") === "true") as HTMLElement;

describe("the list", () => {
  it("reads like the design: heading, how many wait, only today and yesterday, newest first", async () => {
    await open();
    expect(within(screen.getByRole("main")).getByText("Recently kept")).toBeInTheDocument();
    expect(
      screen.getByText(
        "1 waiting to be read. Nocturne reads it tonight, or sooner if the computer is free.",
      ),
    ).toBeInTheDocument();
    const rows = within(screen.getByRole("list", { name: /today and yesterday/ })).getAllByRole(
      "listitem",
    );
    expect(rows[0]).toHaveTextContent("Parking permit renewal");
    expect(rows.length).toBeGreaterThanOrEqual(8);
    // Older things are not here: this is a window, not an archive.
    expect(screen.queryByText("Letter")).toBeNull();
    expect(screen.queryByText("Gym door code")).toBeNull();
  });

  it("each row says what it is, when, and where it came from", async () => {
    await open();
    expect(rowOf("Parking permit renewal")).toHaveTextContent("Photo · 06:41 · quick keep");
    expect(rowOf("Ask Sam about Saturday")).toHaveTextContent("Voice note · 0:06 · 06:15");
    expect(rowOf("Class WhatsApp, 3 messages")).toHaveTextContent("Pasted · 05:58");
    expect(rowOf("Dentist moved")).toHaveTextContent("Voice note · 0:08 · yesterday");
    expect(rowOf("Swimming timetable")).toHaveTextContent("Photo · yesterday");
  });

  it("a watched folder, a web page and a screenshot say where they came from", async () => {
    const engine = new Kept();
    engine.extra = [
      await base(engine, "inbox", {
        kind: "image",
        source: "watchedFolder",
        status: "understood",
        reason: null,
      }),
      await base(engine, "page", {
        kind: "web",
        source: "pastedWeb",
        status: "understood",
        reason: null,
      }),
    ];
    await open(engine);
    expect(rowOf("inbox")).toHaveTextContent("Photo · yesterday · Nocturne inbox folder");
    expect(rowOf("page")).toHaveTextContent("Web page you pasted · yesterday");
  });

  it("each row has its status in words", async () => {
    await open();
    expect(within(rowOf("Parking permit renewal")).getByText("Reading…")).toBeInTheDocument();
    expect(within(rowOf("Ask Sam about Saturday")).getByText("Understood")).toBeInTheDocument();
    expect(within(rowOf("Dentist moved")).getByText("Needs you")).toBeInTheDocument();
    expect(within(rowOf("council-tax-2026.pdf")).getByText("Couldn’t read")).toBeInTheDocument();
  });

  it("a thing being read has no button, a locked file has a sentence instead", async () => {
    await open();
    expect(within(rowOf("Parking permit renewal")).queryByRole("button")).toBeNull();
    const locked = rowOf("council-tax-2026.pdf");
    expect(within(locked).queryByRole("button")).toBeNull();
    expect(locked).toHaveTextContent("Dropped · yesterday · the file is password-locked");
    expect(locked).toHaveTextContent("Save an unlocked copy and drop it here");
  });

  it("the buttons go where the item needs to go, and name the item", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Open: Ask Sam about Saturday" }));
    expect(window.location.hash).toBe("#/items/item-sam");
  });

  it("Which is right? and Confirm take the person to Needs you", async () => {
    const engine = new Kept();
    engine.extra = [
      await base(engine, "Lunchbox recipe ideas", {
        kind: "web",
        source: "pastedWeb",
        status: "unconfirmed",
        reason: null,
      }),
    ];
    await open(engine);
    await userEvent.click(screen.getByRole("button", { name: "Which is right?: Dentist moved" }));
    expect(window.location.hash).toBe("#/needs-you");
    window.location.hash = "#/recent";
    await userEvent.click(
      await screen.findByRole("button", { name: "Confirm: Lunchbox recipe ideas" }),
    );
    expect(window.location.hash).toBe("#/needs-you");
  });
});

describe("what is kept now", () => {
  it("shows a new note with Read now, which reads it at once and keeps focus on the row", async () => {
    const engine = await open();
    const spy = vi.spyOn(engine, "readNow");
    await act(async () => {
      await engine.keep("Buy milk");
    });
    const row = await waitFor(() => rowOf("Buy milk"));
    expect(row).toHaveTextContent("Kept · reads tonight");
    await userEvent.click(within(row).getByRole("button", { name: "Read now: Buy milk" }));
    expect(spy).toHaveBeenCalledOnce();
    await waitFor(() =>
      expect(within(rowOf("Buy milk")).getByText("Reading…")).toBeInTheDocument(),
    );
    expect(rowOf("Buy milk")).toHaveFocus();
  });

  it("announces only what was just kept and the row the person is on", async () => {
    const engine = await open();
    await act(async () => {
      await engine.keep("Buy milk");
    });
    await waitFor(() => expect(announcer()).toHaveTextContent("Kept: Buy milk."));

    // A change to a row nobody is on stays quiet.
    await act(async () => {
      await engine.readNow("item-sam");
    });
    await waitFor(() =>
      expect(within(rowOf("Ask Sam about Saturday")).getByText("Reading…")).toBeInTheDocument(),
    );
    expect(announcer()).toHaveTextContent("Kept: Buy milk.");
    expect(announcer()).not.toHaveTextContent("Ask Sam");

    // A change to the row with focus is said.
    act(() => rowOf("Class WhatsApp, 3 messages").focus());
    await act(async () => {
      await engine.readNow("item-whatsapp");
    });
    await waitFor(() =>
      expect(announcer()).toHaveTextContent("Class WhatsApp, 3 messages: Reading…."),
    );
  });

  it("says so when reading could not start, and keeps the button", async () => {
    const engine = await open();
    await act(async () => {
      await engine.keep("Buy milk");
    });
    vi.spyOn(engine, "readNow").mockRejectedValue(new Error("no"));
    await userEvent.click(await screen.findByRole("button", { name: "Read now: Buy milk" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("That didn’t go through");
    expect(screen.getByRole("button", { name: "Read now: Buy milk" })).toBeInTheDocument();
  });
});

describe("couldn’t read", () => {
  it("offers Try again where trying again can help, and only there", async () => {
    const engine = new Kept();
    engine.extra = [
      await base(engine, "damaged", { reason: "corrupt" }),
      await base(engine, "foreign", { reason: "unsupported" }),
      await base(engine, "huge", { reason: "tooLong" }),
    ];
    await open(engine);
    const spy = vi.spyOn(engine, "retryItem");
    expect(
      within(rowOf("damaged")).getByRole("button", { name: "Try again: damaged" }),
    ).toBeVisible();
    expect(within(rowOf("foreign")).queryByRole("button")).toBeNull();
    expect(rowOf("foreign")).toHaveTextContent("this kind of file isn’t supported");
    expect(rowOf("huge")).toHaveTextContent("it is too long to read");
    await userEvent.click(screen.getByRole("button", { name: "Try again: damaged" }));
    expect(spy).toHaveBeenCalledWith("damaged");
  });
});

describe("states", () => {
  it("is calm when nothing has been kept", async () => {
    const engine = new Kept();
    engine.only = [];
    renderApp({ hash: "#/recent", engine });
    expect(await screen.findByText("Nothing kept yet.")).toBeInTheDocument();
    expect(
      screen.getByText("Type, paste or drop something and it will show up here."),
    ).toBeVisible();
  });

  it("says so in words when the list cannot be read, and tries again", async () => {
    const engine = new Kept();
    engine.fail = true;
    renderApp({ hash: "#/recent", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t get this list. What you kept is safe.",
    );
    engine.fail = false;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Today and yesterday",
    );
  });
});

describe("describe helpers", () => {
  const item = (patch: Partial<ItemView>): ItemView =>
    ({
      id: "x",
      kind: "text",
      title: "x",
      source: "paste",
      status: "understood",
      reason: null,
      durationMs: null,
      fileName: null,
      keptAt: "2026-11-13T05:58:00Z",
      ...patch,
    }) as ItemView;

  it("thumbnails say what a thing is in plain words, never a file ending", () => {
    expect(thumbCode(item({ fileName: "a.pdf", kind: "document" }))).toBe("Document");
    expect(thumbCode(item({ fileName: "memo.m4a", kind: "audio" }))).toBe("Recording");
    expect(thumbCode(item({ fileName: "notes.txt", kind: "text" }))).toBe("Note");
    expect(thumbCode(item({ kind: "web" }))).toBe("Web page");
    expect(thumbCode(item({ kind: "image" }))).toBe("");
    for (const kind of ["text", "audio", "web", "document"] as const) {
      expect(
        thumbCode(item({ kind, fileName: `x.${kind === "audio" ? "m4a" : "pdf"}` })),
      ).not.toMatch(/^[A-Z0-9]{3,4}$/);
    }
  });

  it("a locked file has no action and a kept one reads now", () => {
    expect(actionFor(item({ status: "couldntRead", reason: "locked" }))).toBeNull();
    expect(actionFor(item({ status: "kept" }))).toEqual({ type: "readNow" });
  });

  it("the subline says yesterday for yesterday and the time for today", () => {
    expect(subline(item({}), "2026-11-13")).toBe("Pasted · 05:58");
    expect(subline(item({}), "2026-11-14")).toBe("Pasted · yesterday");
  });
});
