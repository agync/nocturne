import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AskDelta, ItemView } from "../bindings";
import { createMockEngine } from "./engine";
import { SAFETY_PREDICATES } from "./seed";

const NOW = new Date("2026-11-13T07:02:00Z");

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
});
afterEach(() => {
  vi.useRealTimers();
});

function collectAsk(engine: ReturnType<typeof createMockEngine>) {
  const deltas: AskDelta[] = [];
  const off = engine.on("ask-delta", (d) => deltas.push(d));
  return { deltas, off };
}

describe("MockEngine capture", () => {
  it("acknowledges a kept note in under 100 ms and reads it later", async () => {
    const engine = createMockEngine({ now: NOW });
    const changes: ItemView[] = [];
    engine.on("item-changed", (item) => changes.push(item));

    const started = performance.now();
    const kept = await engine.keepText("Ask Sam about Saturday", "quickCapture");
    expect(performance.now() - started).toBeLessThan(100);
    expect(kept.type).toBe("new");
    expect(kept.item.status).toBe("kept");
    expect(kept.item.title).toBe("Ask Sam about Saturday");

    await vi.advanceTimersByTimeAsync(2000);
    expect(changes.at(-1)?.status).toBe("reading");
    await vi.advanceTimersByTimeAsync(6000);
    expect(changes.at(-1)?.status).toBe("understood");
    expect(changes.every((c) => c.id === kept.item.id)).toBe(true);
  });

  it("says kindly when something was already kept", async () => {
    const engine = createMockEngine({ now: NOW });
    await engine.keepText("Bin day is Tuesday", "paste");
    const again = await engine.keepText("Bin day is Tuesday", "paste");
    expect(again.type).toBe("duplicate");
  });

  it("lists what was kept today with the newest first", async () => {
    const engine = createMockEngine({ now: NOW });
    await engine.keepText("A new note", "quickCapture");
    const items = await engine.listItems({
      since: "2026-11-13T00:00:00Z",
      status: null,
      limit: null,
    });
    expect(items[0]?.title).toBe("A new note");
    expect(items.every((i) => i.keptAt >= "2026-11-13T00:00:00Z")).toBe(true);
  });

  it("keeps a web paste as unconfirmed", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepText("Nut-free ideas from a blog", "pastedWeb");
    expect(kept.item.kind).toBe("web");
    expect(kept.item.trust).toBe("untrusted");
  });
});

describe("MockEngine ask", () => {
  it("answers from what was kept, with a receipt, ending in done", async () => {
    const engine = createMockEngine({ now: NOW });
    const { deltas } = collectAsk(engine);
    const id = await engine.ask("When is the dentist?");
    await vi.advanceTimersByTimeAsync(10_000);
    const mine = deltas.filter((d) => d.askId === id);
    expect(mine.at(-1)?.phase).toBe("done");
    expect(mine.flatMap((d) => d.receipts).length).toBeGreaterThan(0);
    expect(mine.map((d) => d.textDelta ?? "").join("")).toContain("Dr Chen");
  });

  it("says nothing was found when nothing matches", async () => {
    const engine = createMockEngine({ now: NOW });
    const { deltas } = collectAsk(engine);
    const id = await engine.ask("passport");
    await vi.advanceTimersByTimeAsync(10_000);
    expect(deltas.filter((d) => d.askId === id).at(-1)?.phase).toBe("nothing");
  });

  it("is honest when it is not sure and offers the near matches", async () => {
    const engine = createMockEngine({ now: NOW });
    const { deltas } = collectAsk(engine);
    const id = await engine.ask("Is Leo allergic to nuts?");
    await vi.advanceTimersByTimeAsync(10_000);
    const last = deltas.filter((d) => d.askId === id).at(-1);
    expect(last?.phase).toBe("notSure");
  });
});

describe("MockEngine forgetting", () => {
  it("previews, then removes the memory from lists and redacts the page line", async () => {
    const engine = createMockEngine({ now: NOW });
    const before = await engine.listMemories({
      itemId: null,
      entityId: null,
      status: null,
      query: null,
      limit: null,
    });
    expect(before.some((m) => m.id === "mem-gran")).toBe(true);

    const preview = await engine.previewForget(["mem-gran"], true);
    expect(preview.memories.map((m) => m.memoryId)).toEqual(["mem-gran"]);
    expect(preview.originals.map((o) => o.itemId)).toEqual(["item-gran"]);
    expect(preview.pagesAffected).toBeGreaterThan(0);

    const report = await engine.forget(preview.token);
    expect(report.memoriesRemoved).toBe(1);
    expect(report.originalsRemoved).toBe(1);

    const after = await engine.listMemories({
      itemId: null,
      entityId: null,
      status: null,
      query: null,
      limit: null,
    });
    expect(after.some((m) => m.id === "mem-gran")).toBe(false);

    const page = await engine.getPage();
    const redacted = page.changed.find((l) => l.memoryIds.includes("mem-gran"));
    expect(redacted?.forgotten).toBe(true);
    expect(redacted?.text).not.toContain("4471");
    expect(redacted?.receipts).toEqual([]);
  });

  it("keeps the original when it is not removed", async () => {
    const engine = createMockEngine({ now: NOW });
    const preview = await engine.previewForget(["mem-gran"], false);
    expect(preview.originals).toEqual([]);
    const report = await engine.forget(preview.token);
    expect(report.originalsRemoved).toBe(0);
    await expect(engine.getItem("item-gran")).resolves.toMatchObject({ id: "item-gran" });
  });
});

describe("MockEngine page", () => {
  it("never asks about a safety memory", async () => {
    const engine = createMockEngine({ now: NOW });
    const page = await engine.getPage();
    const memories = await engine.listMemories({
      itemId: null,
      entityId: null,
      status: null,
      query: null,
      limit: null,
    });
    const asked = memories.find((m) => m.id === page.question?.memoryId);
    expect(asked).toBeDefined();
    expect(SAFETY_PREDICATES.has(asked?.predicate ?? "")).toBe(false);
    expect(asked?.keepForever).toBe(false);
    expect(page.question?.text).toBe("Is Leo still doing swimming with Coach Sam?");
  });

  it("gives every receipt an item that exists", async () => {
    const engine = createMockEngine({ now: NOW });
    const page = await engine.getPage();
    const lines = [...page.needsYou, ...page.changed, ...page.letGo];
    const receipts = lines.flatMap((l) => [...l.receipts, ...l.choices.map((c) => c.receipt)]);
    expect(receipts.length).toBeGreaterThan(0);
    for (const r of receipts) {
      await expect(engine.getItem(r.itemId)).resolves.toMatchObject({ id: r.itemId });
    }
  });

  it("resolves a which-is-right line and lowers the count", async () => {
    const engine = createMockEngine({ now: NOW });
    const counts: number[] = [];
    engine.on("needs-you-changed", (n) => counts.push(n));
    expect((await engine.getStatus()).needsYou).toBe(2);
    const resolved = await engine.resolveWhich("conflict-dentist", "mem-dentist-thu");
    expect(resolved).toEqual({
      subject: "Dentist",
      chosen: expect.stringMatching(/^Thursday \d+ Nov, 10:00$/),
      memoryId: "mem-dentist-thu",
    });
    const page = await engine.getPage();
    expect(page.needsYou.find((l) => l.kind === "which")?.done).toBe(true);
    expect(counts.at(-1)).toBe(1);
    expect(page.headline).toBe("One thing needs you today.");
    const chosen = (
      await engine.listMemories({
        itemId: null,
        entityId: null,
        status: null,
        query: "dentist",
        limit: null,
      })
    ).find((m) => m.id === "mem-dentist-thu");
    expect(chosen?.status).toBe("active");
  });

  it("marks a due line done, brings back a let-go line and records the answer", async () => {
    const engine = createMockEngine({ now: NOW });
    await engine.markDone("mem-trip");
    await engine.restore(["mem-gym"]);
    await engine.answerQuestion((await engine.getPage()).date, "yes");
    const page = await engine.getPage();
    expect(page.needsYou.find((l) => l.id === "line-trip")?.done).toBe(true);
    expect(page.question?.answered).toBe("yes");
    const gym = (
      await engine.listMemories({
        itemId: null,
        entityId: null,
        status: null,
        query: "gym",
        limit: null,
      })
    )[0];
    expect(gym?.status).toBe("active");
  });

  it("lists past pages newest first", async () => {
    const engine = createMockEngine({ now: NOW });
    const pages = await engine.listPages();
    expect(pages[0]?.date).toBe("2026-11-13");
    expect(pages.map((p) => p.date)).toEqual([...pages.map((p) => p.date)].sort().reverse());
  });
});

describe("MockEngine settings and events", () => {
  it("applies a settings patch and defaults update checks off", async () => {
    const engine = createMockEngine({ now: NOW });
    const before = await engine.getSettings();
    expect(before.checkUpdates).toBe(false);
    const after = await engine.updateSettings({
      pageTime: "06:30",
      delivery: null,
      autoPrint: null,
      voice: null,
      voiceSpeed: null,
      shortcut: null,
      inboxFolder: null,
      startAtLogin: null,
      lettingGo: null,
      theme: "dark",
      language: null,
      checkUpdates: null,
      reminders: null,
      lock: null,
      agents: null,
    });
    expect(after.pageTime).toBe("06:30");
    expect(after.theme).toBe("dark");
    expect(after.checkUpdates).toBe(false);
  });

  it("unsubscribes synchronously", async () => {
    const engine = createMockEngine({ now: NOW });
    const seen: number[] = [];
    const off = engine.on("needs-you-changed", (n) => seen.push(n));
    off();
    await engine.markDone("mem-trip");
    expect(seen).toEqual([]);
  });

  it("builds the seed relative to the clock it is given", async () => {
    const engine = createMockEngine({ now: new Date("2026-06-01T07:02:00Z") });
    const page = await engine.getPage();
    expect(page.date).toBe("2026-06-01");
  });
});

describe("MockEngine, shared by the screen lanes", () => {
  it("takes back something just kept", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepText("Take this back", "quickCapture");
    await engine.unkeep(kept.item.id);
    await expect(engine.getItem(kept.item.id)).rejects.toThrow();
  });

  it("keeps pasted bytes by what they are", async () => {
    const engine = createMockEngine({ now: NOW });
    const photo = await engine.keepBytes(new Uint8Array([1]), "image/png", "paste");
    const note = await engine.keepBytes(new Uint8Array(32_000), "audio/webm", "recording");
    expect(photo.item.kind).toBe("image");
    expect(note.item.kind).toBe("audio");
    expect(note.item.durationMs).toBeGreaterThan(1000);
  });

  it("reads an item now and says it is reading", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepText("Read me now", "quickCapture");
    await expect(engine.readNow(kept.item.id)).resolves.toMatchObject({ status: "reading" });
  });

  it("lists people, places and things, filtered", async () => {
    const engine = createMockEngine({ now: NOW });
    expect((await engine.listEntities(null, null)).length).toBeGreaterThan(3);
    expect((await engine.listEntities("person", "okafor")).map((e) => e.id)).toEqual([
      "ent-okafor",
    ]);
    expect(await engine.listEntities("place", null)).toEqual([]);
  });

  it("snoozes a fading memory back to active, and flags items", async () => {
    const engine = createMockEngine({ now: NOW });
    await engine.snooze("mem-jo", 30);
    const jo = (
      await engine.listMemories({
        itemId: null,
        entityId: null,
        status: null,
        query: "Jo",
        limit: null,
      })
    )[0];
    expect(jo?.status).toBe("active");
    await engine.setItemPrivate("item-gran", true);
    expect((await engine.getItem("item-gran")).private).toBe(true);
    await engine.setItemKeepForever("item-gran", true);
    const gran = await engine.listMemories({
      itemId: "item-gran",
      entityId: null,
      status: null,
      query: null,
      limit: null,
    });
    expect(gran.every((m) => m.keepForever)).toBe(true);
  });

  it("filters memories by the item they came from", async () => {
    const engine = createMockEngine({ now: NOW });
    const out = await engine.listMemories({
      itemId: "item-letter",
      entityId: null,
      status: null,
      query: null,
      limit: null,
    });
    expect(out.map((m) => m.id)).toContain("mem-trip");
    expect(out.every((m) => m.receipts.some((r) => r.itemId === "item-letter"))).toBe(true);
  });

  it("exports with progress and reports where it wrote", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const engine = createMockEngine({ now: NOW });
    const seen: number[] = [];
    engine.on("export-progress", (p) => seen.push(p.done / p.total));
    const report = await engine.exportTo("/out");
    expect(report.path).toBe("/out/Nocturne export");
    expect(report.memories).toBeGreaterThan(0);
    await vi.advanceTimersByTimeAsync(5000);
    expect(seen.at(-1)).toBe(1);
    vi.useRealTimers();
  });
});
