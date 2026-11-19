import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { splitQuotes } from "./Original";
import { canDecode, computePeaks, MAX_DECODE_MS } from "./peaks";

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
    expect(screen.getByText("Photo · Wed 11 Nov 18:12")).toBeInTheDocument();
    expect(
      within(screen.getByRole("main")).getByRole("link", { name: /What Nocturne remembers/ }),
    ).toHaveAttribute("href", "#/memories");
    expect(screen.getByText("Understood in 4 seconds")).toBeInTheDocument();
    expect(screen.getByText("What it says")).toBeInTheDocument();
    expect(screen.getByText(/A letter from Willow Primary/)).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Photo you kept: Letter" })).toHaveAttribute(
      "src",
      "file:originals/item-letter",
    );
  });

  it("says Unconfirmed before how long it took to understand", async () => {
    const engine = createMockEngine();
    const real = engine.getItem.bind(engine);
    vi.spyOn(engine, "getItem").mockImplementation(async (itemId) => ({
      ...(await real(itemId)),
      understoodInMs: 3000,
    }));
    await open("item-nut", engine);
    const unconfirmed = await screen.findByText("Unconfirmed", { selector: "span" });
    const understood = screen.getByText("Understood in 3 seconds");
    expect(
      unconfirmed.compareDocumentPosition(understood) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
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

  it("decodes a short recording for its waveform but never a very long one", async () => {
    const fetchSpy = vi.fn().mockRejectedValue(new Error("blocked"));
    vi.stubGlobal("fetch", fetchSpy);
    vi.stubGlobal("OfflineAudioContext", class {});
    const platform = fakePlatform();
    vi.spyOn(platform, "originalSrc").mockReturnValue("blob:recording");
    await open("item-lake", createMockEngine(), platform);
    await waitFor(() => expect(fetchSpy).toHaveBeenCalledWith("blob:recording"));
    cleanup();
    fetchSpy.mockClear();
    const engine = createMockEngine();
    const real = engine.getItem.bind(engine);
    vi.spyOn(engine, "getItem").mockImplementation(async (id) => ({
      ...(await real(id)),
      durationMs: 4 * 3600_000,
    }));
    await open("item-lake", engine, platform);
    await screen.findByRole("button", { name: "Play" });
    expect(fetchSpy).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("says so when a recording cannot be played here", async () => {
    vi.spyOn(HTMLMediaElement.prototype, "play").mockRejectedValue(new Error("no"));
    await open("item-lake");
    await userEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(await screen.findByText(/Nocturne can’t play this here/)).toBeInTheDocument();
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
    const spy = vi.spyOn(platform, "openOriginal");
    await open("item-letter", createMockEngine(), platform);
    await userEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(spy).toHaveBeenCalledWith("item-letter", expect.any(String));
  });

  it("says so when the original cannot be opened", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "openOriginal").mockRejectedValue(new Error("gone"));
    await open("item-letter", createMockEngine(), platform);
    await userEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(
      await screen.findByText("That didn’t work. Nothing was lost; try again in a moment."),
    ).toBeInTheDocument();
  });

  it("forgets the original and its memories, then leaves for the list", async () => {
    await open("item-gran");
    await userEvent.click(screen.getByRole("button", { name: "Delete…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(await within(dialog).findByRole("button", { name: /^Delete 2 things/ }));
    expect(await within(dialog).findByText("Deleted.")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(window.location.hash).toBe("#/memories"));
  });

  it("deletes a check-in, which has no memories of its own", async () => {
    const engine = createMockEngine({
      now: new Date("2026-11-19T21:20:00Z"),
      journal: { mode: "day", delayMs: 0 },
    });
    await open("j-ci-tired", engine);
    const button = screen.getByRole("button", { name: "Delete…" });
    expect(button).toBeEnabled();
    await userEvent.click(button);
    const dialog = await screen.findByRole("dialog", { name: "Delete 1 thing?" });
    await userEvent.click(await within(dialog).findByRole("button", { name: "Delete 1 thing" }));
    expect(await within(dialog).findByText("Deleted.")).toBeInTheDocument();
    expect((await engine.getJournalDay()).checkins.map((c) => c.id)).not.toContain("j-ci-tired");
  });

  it("explains why something could not be read and offers to read it again", async () => {
    const { engine } = await open("item-counciltax");
    expect(screen.getByText(/password-locked/)).toBeInTheDocument();
    const spy = vi.spyOn(engine, "retryItem");
    await userEvent.click(screen.getByRole("button", { name: "Read it again" }));
    expect(spy).toHaveBeenCalledWith("item-counciltax");
  });

  it("says so for an item that is gone", async () => {
    renderApp({ hash: "#/items/nothing" });
    expect(await screen.findByRole("alert")).toHaveTextContent("Nocturne couldn’t find that.");
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

  it("does not try to decode a very long recording for its waveform", () => {
    expect(canDecode(42_000)).toBe(true);
    expect(canDecode(null)).toBe(true);
    expect(canDecode(MAX_DECODE_MS)).toBe(true);
    expect(canDecode(MAX_DECODE_MS + 1)).toBe(false);
  });

  it("finds the loudest part of each slice and scales to one", () => {
    const samples = new Float32Array([0, 0.1, 0.5, -0.25, 0, 0, 0.05, -0.1]);
    const peaks = computePeaks(samples, 4);
    expect(peaks.map((p) => Number(p.toFixed(3)))).toEqual([0.2, 1, 0, 0.2]);
    expect(computePeaks(new Float32Array(0), 4)).toEqual([]);
    expect(computePeaks(new Float32Array(8), 4)).toEqual([0, 0, 0, 0]);
  });
});

describe("a journal item", () => {
  it("does not offer Mark private, and an ordinary thing still does", async () => {
    const engine = createMockEngine();
    const ordinary = await engine.getItem("item-letter");
    const original = engine.getItem.bind(engine);
    vi.spyOn(engine, "getItem").mockImplementation(async (id) => ({
      ...(await original(id)),
      journal: id === "item-letter" ? "entry" : undefined,
    }));
    await open("item-letter", engine);
    expect(screen.getByRole("button", { name: "Keep forever" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Mark private" })).toBeNull();
    expect(ordinary.journal).toBeUndefined();
    cleanup();
    await open("item-card", createMockEngine());
    expect(screen.getByRole("button", { name: "Mark private" })).toBeVisible();
  });
});
