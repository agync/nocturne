import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { splitQuotes } from "./Original";
import { computePeaks } from "./peaks";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const open = async (id: string, engine = createMockEngine(), platform = fakePlatform()) => {
  const view = renderApp({ hash: `#/items/${id}`, engine, platform });
  await screen.findByRole("heading", { level: 1 });
  await screen.findByRole("heading", { level: 2, name: /^Memories from it/ });
  return { ...view, engine, platform };
};

describe("an original", () => {
  it("says what it is, where it came from and how long it took to understand", async () => {
    await open("item-letter");
    expect(screen.getByRole("heading", { level: 1, name: "Letter" })).toBeVisible();
    expect(screen.getByText("Photo · Kept Wed 11 Nov 18:12 · Dropped in")).toBeInTheDocument();
    expect(screen.getByText("Understood in 4 seconds")).toBeInTheDocument();
    expect(screen.getByText("What it says")).toBeInTheDocument();
    expect(screen.getByText(/A letter from Willow Primary/)).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Photo you kept: Letter" })).toHaveAttribute(
      "src",
      "file:originals/item-letter",
    );
  });

  it("lists the memories from it, with a Needs you marker for unconfirmed ones", async () => {
    await open("item-letter");
    const from = screen.getByRole("region", { name: "Memories from it · 4" });
    expect(within(from).getAllByRole("listitem")).toHaveLength(4);
    expect(
      within(from).getByRole("link", { name: "Leo’s teacher is Mrs Okafor." }),
    ).toHaveAttribute("href", "#/memories/ent-okafor");
  });

  it("marks an unconfirmed memory as needing you", async () => {
    await open("item-nut");
    const nut = screen.getByRole("region", { name: "Memories from it · 1" });
    expect(within(nut).getByText("Needs you")).toBeInTheDocument();
  });

  it("highlights quotes in words, and the highlight has a text equivalent", async () => {
    await open("item-nut");
    expect(screen.getByText("Highlighted:", { exact: false })).toBeInTheDocument();
    const mark = document.querySelector("mark");
    expect(mark).toHaveTextContent("Leo is allergic to nuts so I always");
  });

  it("draws highlighted regions of a photo with their sentences as names", async () => {
    const engine = createMockEngine();
    const original = engine.listMemories.bind(engine);
    vi.spyOn(engine, "listMemories").mockImplementation(async (filter) => {
      const found = await original(filter);
      return found.map((m) =>
        m.id === "mem-trip"
          ? {
              ...m,
              receipts: m.receipts.map((r) => ({
                ...r,
                span: { type: "region" as const, x: 0.1, y: 0.2, w: 0.5, h: 0.1 },
              })),
            }
          : m,
      );
    });
    await open("item-letter", engine);
    expect(
      screen.getByRole("img", { name: /^Highlighted in the photo: The Year 4 trip form is due/ }),
    ).toBeInTheDocument();
  });

  it("plays a recording and shows where its memories come from", async () => {
    const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
    await open("item-lake");
    expect(screen.getByText("What was said")).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Position in the recording" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "0:00 to 0:42: You decided to book the Lake District cottage for half term.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(play).toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("says so when a recording cannot be played here", async () => {
    vi.spyOn(HTMLMediaElement.prototype, "play").mockRejectedValue(new Error("no"));
    await open("item-lake");
    await userEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(await screen.findByText(/I can’t play this here/)).toBeInTheDocument();
  });

  it("marks the original private and keeps its memories forever", async () => {
    const { engine } = await open("item-gran");
    const priv = vi.spyOn(engine, "setItemPrivate");
    const keep = vi.spyOn(engine, "setItemKeepForever");
    await userEvent.click(screen.getByRole("button", { name: "Mark private" }));
    expect(priv).toHaveBeenCalledWith("item-gran", true);
    expect(await screen.findByText("Marked private.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep forever" }));
    expect(keep).toHaveBeenCalledWith("item-gran", true);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Keep forever" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  it("reads it again on request", async () => {
    const { engine } = await open("item-gran");
    const spy = vi.spyOn(engine, "readNow");
    await userEvent.click(screen.getByRole("button", { name: "Read it again" }));
    expect(spy).toHaveBeenCalledWith("item-gran");
    expect(await screen.findByText("Reading it again.")).toBeInTheDocument();
  });

  it("opens the stored original with the system's own app", async () => {
    const platform = fakePlatform();
    const spy = vi.spyOn(platform, "openPath");
    await open("item-letter", createMockEngine(), platform);
    await userEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(spy).toHaveBeenCalledWith("originals/item-letter");
  });

  it("forgets the original and its memories, then leaves for the list", async () => {
    await open("item-gran");
    await userEvent.click(screen.getByRole("button", { name: "Forget…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Forget 2 things/ }));
    expect(await within(dialog).findByText("Gone.")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(window.location.hash).toBe("#/memories"));
  });

  it("explains why something could not be read and offers to try again", async () => {
    const { engine } = await open("item-counciltax");
    expect(screen.getByText(/password-locked/)).toBeInTheDocument();
    const spy = vi.spyOn(engine, "retryItem");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(spy).toHaveBeenCalledWith("item-counciltax");
  });

  it("says so for an item that is gone", async () => {
    renderApp({ hash: "#/items/nothing" });
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t find that.");
  });
});

describe("helpers", () => {
  it("splits text around the quotes to highlight", () => {
    expect(splitQuotes("Pay £12 by Friday, please.", ["£12", "Friday"])).toEqual([
      { text: "Pay ", mark: false },
      { text: "£12", mark: true },
      { text: " by ", mark: false },
      { text: "Friday", mark: true },
      { text: ", please.", mark: false },
    ]);
    expect(splitQuotes("Nothing here", ["zebra"])).toEqual([{ text: "Nothing here", mark: false }]);
  });

  it("finds the loudest part of each slice and scales to one", () => {
    const samples = new Float32Array([0, 0.1, 0.5, -0.25, 0, 0, 0.05, -0.1]);
    const peaks = computePeaks(samples, 4);
    expect(peaks.map((p) => Number(p.toFixed(3)))).toEqual([0.2, 1, 0, 0.2]);
    expect(computePeaks(new Float32Array(0), 4)).toEqual([]);
    expect(computePeaks(new Float32Array(8), 4)).toEqual([0, 0, 0, 0]);
  });
});
