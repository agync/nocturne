import { statSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AskDelta, ItemView } from "../bindings";
import recorded from "./assets/spoken-today.json";
import { createMockEngine } from "./engine";
import { audioScript } from "./script";
import { SAFETY_PREDICATES } from "./seed";

const NOW = new Date("2026-11-13T07:02:00Z");

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
});
afterEach(() => {
  vi.useRealTimers();
});

const filterFor = (entityId: string) => ({
  entityId,
  itemId: null,
  status: null,
  query: null,
  limit: null,
});

function collectAsk(engine: ReturnType<typeof createMockEngine>) {
  const deltas: AskDelta[] = [];
  const off = engine.on("ask-delta", (d) => deltas.push(d));
  return { deltas, off };
}

describe("MockEngine capture", () => {
  it("keeps a voice note as a WAV and holds the sound to play back", async () => {
    const create = vi.fn(() => "blob:held");
    const before = URL.createObjectURL;
    URL.createObjectURL = create;
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepRecording(new Uint8Array(44 + 96_000), "audio/wav");
    expect(kept.type).toBe("new");
    expect(kept.item.kind).toBe("audio");
    expect(kept.item.source).toBe("recording");
    expect(kept.item.durationMs).toBe(3000);
    expect(kept.item.originalPath).toBe("blob:held");
    expect((await engine.getItem(kept.item.id)).originalPath).toBe("blob:held");
    URL.createObjectURL = before;
  });

  it("refuses a recording in a form the speech model cannot read", async () => {
    const engine = createMockEngine({ now: NOW });
    for (const mime of ["audio/webm;codecs=opus", "audio/mp4", "audio/ogg"]) {
      await expect(engine.keepRecording(new Uint8Array(32_000), mime)).rejects.toThrow(/WAV/);
    }
    expect(await engine.listItems({ since: null, status: null, limit: null })).not.toContainEqual(
      expect.objectContaining({
        source: "recording",
        title: "Voice note",
        id: expect.stringMatching(/^new-/),
      }),
    );
  });

  it("acknowledges a kept note in under 100 ms and leaves it waiting for tonight", async () => {
    const engine = createMockEngine({ now: NOW });
    const changes: ItemView[] = [];
    engine.on("item-changed", (item) => changes.push(item));

    const started = performance.now();
    const kept = await engine.keepText("Ask Sam about Saturday", "quickCapture");
    expect(performance.now() - started).toBeLessThan(100);
    expect(kept.type).toBe("new");
    expect(kept.item.status).toBe("kept");
    expect(kept.item.title).toBe("Ask Sam about Saturday");

    // There is no mind in the preview: nothing is read, so nothing is claimed.
    await vi.advanceTimersByTimeAsync(10_000);
    const later = await engine.getItem(kept.item.id);
    expect(later).toMatchObject({ status: "kept", memoryCount: 0, understoodInMs: null });
    expect((await engine.getStatus()).state).not.toBe("reading");
  });

  it("claims only the memories it holds: a pasted web page leaves one unconfirmed", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepText("Free parking on Sundays.", "pastedWeb");
    await vi.advanceTimersByTimeAsync(10_000);
    const later = await engine.getItem(kept.item.id);
    expect(later.status).toBe("unconfirmed");
    const held = await engine.listMemories({
      entityId: null,
      itemId: kept.item.id,
      status: null,
      query: null,
      limit: null,
    });
    expect(later.memoryCount).toBe(held.length);
    expect(held.length).toBeGreaterThan(0);
  });

  it("lets go of the held sound when a voice note is taken back", async () => {
    const revoke = vi.fn();
    const before = [URL.createObjectURL, URL.revokeObjectURL];
    URL.createObjectURL = () => "blob:held-1";
    URL.revokeObjectURL = revoke;
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepRecording(new Uint8Array(32_000), "audio/wav");
    await engine.unkeep(kept.item.id);
    expect(revoke).toHaveBeenCalledWith("blob:held-1");
    [URL.createObjectURL, URL.revokeObjectURL] = before as [
      typeof URL.createObjectURL,
      typeof URL.revokeObjectURL,
    ];
  });

  it("lets go of every held sound when everything is deleted", async () => {
    const revoke = vi.fn();
    const before = [URL.createObjectURL, URL.revokeObjectURL];
    let n = 0;
    URL.createObjectURL = () => `blob:held-${++n}`;
    URL.revokeObjectURL = revoke;
    const engine = createMockEngine({ now: NOW });
    await engine.keepRecording(new Uint8Array(32_000), "audio/wav");
    await engine.keepBytes(new Uint8Array([1, 2]), "image/png", "paste");
    await engine.deleteEverything("delete everything");
    expect(revoke).toHaveBeenCalledWith("blob:held-1");
    expect(revoke).toHaveBeenCalledWith("blob:held-2");
    [URL.createObjectURL, URL.revokeObjectURL] = before as [
      typeof URL.createObjectURL,
      typeof URL.revokeObjectURL,
    ];
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

describe("MockEngine memories", () => {
  it("lists a memory that is not yet confirmed when no status is asked for", async () => {
    const engine = createMockEngine();
    const all = await engine.listMemories({
      entityId: null,
      itemId: null,
      status: null,
      query: null,
      limit: null,
    });
    expect(all.find((m) => m.id === "mem-nut")?.status).toBe("unconfirmed");
  });
});

/** Everything one question says, and the last step it reached. */
async function answerTo(question: string, options: { emphasis?: boolean } = {}) {
  const engine = createMockEngine({ now: NOW, ...options });
  const { deltas } = collectAsk(engine);
  const id = await engine.ask(question);
  await vi.advanceTimersByTimeAsync(10_000);
  const mine = deltas.filter((d) => d.askId === id);
  const last = mine.at(-1);
  return {
    phase: last?.phase,
    text: mine.map((d) => d.textDelta ?? "").join(""),
    receipts: mine.flatMap((d) => d.receipts),
    memoryIds: last?.memoryIds ?? [],
    near: mine.flatMap((d) => d.near),
  };
}

const bold = (text: string) => [...text.matchAll(/\*\*(.+?)\*\*/g)].map((m) => m[1]);

describe("MockEngine ask", () => {
  it("answers from what was kept, with a receipt, ending in done", async () => {
    const answer = await answerTo("What is Gran’s new number?");
    expect(answer.phase).toBe("done");
    expect(answer.receipts.length).toBeGreaterThan(0);
    expect(answer.text).toBe("Gran’s new number ends **4471**.");
  });

  it("says when the dentist is only if it is sure: two kept times disagree, so it is not sure", async () => {
    const answer = await answerTo("When is the dentist?");
    expect(answer.phase).toBe("notSure");
    expect(answer.near).toHaveLength(2);
    expect(answer.near.map((n) => n.text).join(" ")).not.toContain("Dr Chen");
  });

  it("answers the half term example from the voice note", async () => {
    const answer = await answerTo("What did we decide about half term?");
    expect(answer.phase).toBe("done");
    expect(answer.text).toBe("You decided to book the **Lake District cottage** for half term.");
    expect(answer.receipts.map((r) => r.itemId)).toEqual(["item-lake"]);
  });

  it.each(["Who is Leo?", "who’s Leo", "Tell me about Leo", "What about Leo?"])(
    "answers %s with who Leo is first, then what else was kept, each with a receipt",
    async (question) => {
      const answer = await answerTo(question);
      expect(answer.phase).toBe("done");
      expect(answer.text.startsWith("Leo is **your son**.")).toBe(true);
      expect(answer.memoryIds).toEqual(["mem-leo-son", "mem-swim", "mem-trip-lunch"]);
      expect(answer.receipts.map((r) => r.itemId)).toEqual([
        "item-leo-son",
        "item-swim",
        "item-letter",
      ]);
      expect(bold(answer.text)).toHaveLength(answer.memoryIds.length);
    },
  );

  it("never states a role without a memory that says it", async () => {
    const answer = await answerTo("Who is Gran?");
    expect(answer.phase).toBe("done");
    expect(answer.text).not.toMatch(/mother/i);
    expect(answer.memoryIds).not.toContain("mem-leo-son");
    expect(answer.memoryIds.length).toBeGreaterThan(0);
    expect(answer.receipts.length).toBeGreaterThanOrEqual(answer.memoryIds.length);
  });

  it("does not use a private memory to say who someone is", async () => {
    const answer = await answerTo("Who is Gran?");
    expect(answer.text).not.toContain("amlodipine");
  });

  it("bolds one phrase per cited memory in citation order, and none with emphasis off", async () => {
    const answer = await answerTo("Who is Leo?");
    expect(bold(answer.text)).toEqual(["your son", "Mondays at 17:30", "packed lunch"]);
    const plain = await answerTo("Who is Leo?", { emphasis: false });
    expect(plain.text).not.toContain("*");
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
    expect((await engine.getStatus()).needsYou).toBe(3);
    const resolved = await engine.resolveWhich("conflict-dentist", "mem-dentist-thu");
    expect(resolved).toEqual({
      subject: "Dentist",
      chosen: expect.stringMatching(/^Thursday \d+ Nov, 10:00$/),
      memoryId: "mem-dentist-thu",
    });
    const page = await engine.getPage();
    expect(page.needsYou.find((l) => l.kind === "which")?.done).toBe(true);
    expect(counts.at(-1)).toBe(2);
    expect(page.headline).toBe("Two things need you today.");
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

  it("only takes back what was kept in the last ten minutes", async () => {
    const engine = createMockEngine({ now: NOW });
    await expect(engine.unkeep("item-letter")).rejects.toThrow(/too long ago/);
    await expect(engine.getItem("item-letter")).resolves.toBeDefined();
  });

  it("says a file kept twice is a duplicate and adds nothing", async () => {
    const engine = createMockEngine({ now: NOW });
    const before = (await engine.listItems({ since: null, status: null, limit: null })).length;
    const [again, fresh] = await engine.keepFiles(
      ["/somewhere/council-tax-2026.pdf", "/somewhere/new-scan.jpg"],
      "drop",
    );
    expect(again?.type).toBe("duplicate");
    expect(again?.item.id).toBe("item-counciltax");
    expect(fresh?.type).toBe("new");
    const after = (await engine.listItems({ since: null, status: null, limit: null })).length;
    expect(after).toBe(before + 1);
  });

  it("a pasted web page, once read, waits on Needs you as unconfirmed and can be left", async () => {
    const engine = createMockEngine({ now: NOW });
    const before = (await engine.getStatus()).needsYou;
    const kept = await engine.keepText("Quick lunchbox recipes", "pastedWeb");
    await vi.advanceTimersByTimeAsync(6000);
    const waiting = (await engine.listNeedsYou()).find((e) => e.type === "unconfirmed");
    expect(waiting?.type === "unconfirmed" && waiting.memory.receipts[0]?.itemId).toBe(
      kept.item.id,
    );
    expect((await engine.getStatus()).needsYou).toBe(before + 1);
    if (waiting?.type !== "unconfirmed") throw new Error("expected an unconfirmed entry");
    await engine.snooze(waiting.memory.id, 7);
    expect((await engine.listNeedsYou()).some((e) => e.type === "unconfirmed")).toBe(false);
    expect((await engine.getStatus()).needsYou).toBe(before);
  });

  it("keeps pasted bytes by what they are", async () => {
    const engine = createMockEngine({ now: NOW });
    const photo = await engine.keepBytes(new Uint8Array([1]), "image/png", "paste");
    const note = await engine.keepBytes(new Uint8Array(32_000), "audio/webm", "recording");
    expect(photo.item.kind).toBe("image");
    expect(note.item.kind).toBe("audio");
    expect(note.item.durationMs).toBeGreaterThan(1000);
  });

  it("reads a note now and remembers it in the person's own words", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepText("Read me now", "quickCapture");
    await expect(engine.readNow(kept.item.id)).resolves.toMatchObject({ status: "reading" });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await engine.getItem(kept.item.id)).toMatchObject({
      status: "understood",
      memoryCount: 1,
    });
    const held = await engine.listMemories({
      entityId: null,
      itemId: kept.item.id,
      status: null,
      query: null,
      limit: null,
    });
    expect(held.map((m) => m.sentence)).toEqual(["Read me now."]);
  });

  it("reads a voice note now and claims nothing, because the preview cannot hear it", async () => {
    const engine = createMockEngine({ now: NOW });
    const kept = await engine.keepRecording(new Uint8Array(32_000), "audio/wav");
    await engine.readNow(kept.item.id);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await engine.getItem(kept.item.id)).toMatchObject({
      status: "kept",
      memoryCount: 0,
      understoodInMs: null,
      text: null,
    });
  });

  it("withdraws a standing offer of a smaller mind when it is declined or taken", async () => {
    const notice = {
      id: "n",
      code: "tierDropped" as const,
      itemId: null,
      etaSeconds: null,
      bytes: 1,
    };
    const declined = createMockEngine({ now: NOW, notices: [notice], tierOffer: "small" });
    await expect(declined.getTierOffer()).resolves.toBe("small");
    await declined.declineTierOffer();
    await expect(declined.getTierOffer()).resolves.toBeNull();
    expect((await declined.getStatus()).notices).toEqual([]);

    const taken = createMockEngine({ now: NOW, notices: [notice], tierOffer: "small" });
    await taken.startSetup("small");
    expect((await taken.getStatus()).notices).toEqual([]);
  });

  it("lists people, places and things, filtered", async () => {
    const engine = createMockEngine({ now: NOW });
    expect((await engine.listEntities(null, null)).length).toBeGreaterThan(3);
    expect((await engine.listEntities("person", "okafor")).map((e) => e.id)).toEqual([
      "ent-okafor",
    ]);
    expect((await engine.listEntities("place", null)).map((e) => e.id)).toEqual([
      "ent-ridgeway",
      "ent-rowan",
    ]);
  });

  it("renames, finds by the old name, and takes the rename back", async () => {
    const engine = createMockEngine({ now: NOW });
    const out = await engine.renameEntity("ent-leo", "  Leopold ");
    if (out.type !== "renamed" || out.changeId === null) throw new Error("not renamed");
    expect(out.entity).toMatchObject({ name: "Leopold", formerNames: ["Leo"] });
    expect(out.entity.lastChange).toMatchObject({ kind: "rename", from: "Leo" });
    const swim = (await engine.listMemories(filterFor("ent-leo"))).find((m) => m.id === "mem-swim");
    expect(swim).toMatchObject({
      subject: "Leopold",
      sentence: "Leopold’s swimming is on Mondays at 17:30.",
    });
    expect((await engine.listEntities(null, "leo ")).map((e) => e.name)).toContain("Leopold");
    expect(await engine.renameEntity("ent-leo", "Leopold")).toMatchObject({ changeId: null });

    await engine.undoChange(out.changeId);
    const back = await engine.getEntity("ent-leo");
    expect(back).toMatchObject({ name: "Leo", formerNames: [], lastChange: null });
    expect((await engine.listMemories(filterFor("ent-leo")))[0]?.subject).toBe("Leo");
    await expect(engine.undoChange(out.changeId)).rejects.toThrow();
  });

  it("offers a merge for a name it knows, and merges and unmerges", async () => {
    const engine = createMockEngine({ now: NOW });
    const clash = await engine.renameEntity("ent-leo", "ava");
    expect(clash).toMatchObject({ type: "exists", entity: { id: "ent-ava" } });
    expect((await engine.getEntity("ent-leo")).name).toBe("Leo");
    await expect(engine.renameEntity("ent-leo", "  ")).rejects.toThrow();

    const leo = await engine.getEntity("ent-leo");
    const ava = await engine.getEntity("ent-ava");
    const merged = await engine.mergeEntities("ent-leo", "ent-ava");
    expect(merged.moved).toBe(leo.memoryCount);
    expect(merged.entity.memoryCount).toBe(leo.memoryCount + ava.memoryCount);
    expect(merged.entity.formerNames).toContain("Leo");
    await expect(engine.getEntity("ent-leo")).rejects.toThrow();
    expect((await engine.listEntities(null, null)).map((e) => e.id)).not.toContain("ent-leo");
    await expect(engine.mergeEntities("ent-ava", "ent-ava")).rejects.toThrow();

    await engine.undoChange(merged.changeId);
    expect(await engine.getEntity("ent-leo")).toMatchObject({ memoryCount: leo.memoryCount });
    expect(await engine.getEntity("ent-ava")).toMatchObject({
      memoryCount: ava.memoryCount,
      formerNames: [],
      lastChange: null,
    });
  });

  it("moves one memory to an existing or a new thing, and moves it back", async () => {
    const engine = createMockEngine({ now: NOW });
    const toAva = await engine.refileMemory("mem-nut", { type: "existing", entityId: "ent-ava" });
    expect(toAva.memory).toMatchObject({
      entityId: "ent-ava",
      subject: "Ava",
      movedFrom: "Leo",
      sentence: "Leo is allergic to nuts.",
    });
    await expect(
      engine.refileMemory("mem-nut", { type: "existing", entityId: "ent-ava" }),
    ).rejects.toThrow();
    const fresh = await engine.refileMemory("mem-nut", {
      type: "new",
      name: " School  trip ",
      kind: "place",
    });
    expect(fresh.entity).toMatchObject({ name: "School trip", kind: "place", memoryCount: 1 });
    expect(
      (
        await engine.refileMemory("mem-trip-lunch", {
          type: "new",
          name: "school trip",
          kind: "thing",
        })
      ).entity.id,
    ).toBe(fresh.entity.id);
    await engine.undoChange(fresh.changeId);
    expect((await engine.listEntities(null, "school")).length).toBe(1);
    expect((await engine.listMemories(filterFor("ent-ava"))).map((m) => m.id)).toContain("mem-nut");
    await engine.undoChange(toAva.changeId);
    expect((await engine.listMemories(filterFor("ent-leo"))).map((m) => m.id)).toContain("mem-nut");
    expect(
      (await engine.listMemories(filterFor("ent-leo"))).find((m) => m.id === "mem-nut")?.movedFrom,
    ).toBeNull();
    await expect(
      engine.refileMemory("mem-nut", { type: "new", name: " ", kind: "thing" }),
    ).rejects.toThrow();
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

describe("MockEngine editions", () => {
  it("leaves lines about private memories out of the spoken and printed pages", async () => {
    const engine = createMockEngine({ now: NOW });
    const screen = await engine.getEdition("2026-11-13", "screen");
    const line = screen.needsYou.find((l) => l.memoryIds.length > 0);
    expect(line).toBeDefined();
    await engine.setPrivate(line?.memoryIds[0] as string, true);

    for (const edition of ["audio", "print"] as const) {
      const page = await engine.getEdition("2026-11-13", edition);
      expect(page.edition).toBe(edition);
      expect(page.needsYou.map((l) => l.id)).not.toContain(line?.id);
      expect(page.privateSkipped).toBeGreaterThan(screen.privateSkipped);
    }
    const audio = await engine.getEdition("2026-11-13", "audio");
    expect(audio.headline).toBe("Friday 13 November");
    expect(audio.dek).toMatch(/^Last night Nocturne read \d+ things? you kept\. \d+ needs? you\./);
    expect(audio.headline).not.toBe(screen.headline);
    expect((await engine.getEdition("2026-11-13", "screen")).needsYou.map((l) => l.id)).toContain(
      line?.id,
    );
  });

  it("speaks the page in sections that start in order and counts what it skipped", async () => {
    const engine = createMockEngine({ now: NOW });
    const spoken = await engine.speakPage("2026-11-13");
    expect(spoken.sections[0]).toEqual({ kind: "opening", startMs: 0 });
    const starts = spoken.sections.map((s) => s.startMs);
    expect([...starts].sort((a, b) => a - b)).toEqual(starts);
    expect(spoken.durationMs).toBeGreaterThan(starts.at(-1) as number);
    expect(spoken.path.length).toBeGreaterThan(0);
  });

  it("notes that the sound settings were asked for", async () => {
    const engine = createMockEngine({ now: NOW });
    await engine.openSoundSettings();
    expect(engine.soundSettingsOpened).toBe(1);
    expect(await engine.quickKeepCommand()).toContain("--quick-capture");
  });
});

describe("MockEngine print check page", () => {
  it("has a heaviest page for the one-sheet check, which is not one of the pages", async () => {
    const engine = createMockEngine({ now: NOW });
    const page = await engine.getEdition("2026-01-01", "print");
    expect(page.edition).toBe("print");
    expect(page.needsYou.length).toBeGreaterThan(6);
    expect(page.changed.length).toBeGreaterThan(6);
    expect(page.question).not.toBeNull();
    expect((await engine.listPages()).map((p) => p.date)).not.toContain("2026-01-01");
  });
});

describe("MockEngine fidelity for ask, needs you and capture", () => {
  const phases = (deltas: AskDelta[]) => deltas.map((d) => d.phase);

  it("mock.ask.waking_first_only", async () => {
    const engine = createMockEngine({ now: NOW, cold: true });
    const { deltas, off } = collectAsk(engine);
    await engine.ask("Is Leo allergic to nuts?");
    await vi.advanceTimersByTimeAsync(2000);
    expect(phases(deltas)[0]).toBe("waking");
    expect(deltas[0]?.etaSeconds).toBeGreaterThan(0);
    expect(phases(deltas)).toContain("searching");
    deltas.length = 0;
    await engine.ask("Is Ava allergic to penicillin?");
    await vi.advanceTimersByTimeAsync(2000);
    expect(phases(deltas)).not.toContain("waking");
    off();
    const warm = createMockEngine({ now: NOW });
    const seen = collectAsk(warm);
    await warm.ask("Is Ava allergic to penicillin?");
    await vi.advanceTimersByTimeAsync(2000);
    expect(phases(seen.deltas)).not.toContain("waking");
  });

  it("mock.ask.bolds_the_key_phrase", async () => {
    const engine = createMockEngine({ now: NOW, emphasis: true });
    const { deltas } = collectAsk(engine);
    await engine.ask("Is Ava allergic to penicillin?");
    await vi.advanceTimersByTimeAsync(2000);
    const text = deltas.map((d) => d.textDelta ?? "").join("");
    expect(text).toContain("**penicillin**");
  });

  it("mock.ask.unavailable_switch", async () => {
    const engine = createMockEngine({ now: NOW });
    engine.setAskUnavailable(true);
    const { deltas } = collectAsk(engine);
    await engine.ask("Is Ava allergic to penicillin?");
    await vi.advanceTimersByTimeAsync(2000);
    expect(phases(deltas)).toEqual(["unavailable"]);
    engine.setAskUnavailable(false);
    deltas.length = 0;
    await engine.ask("Is Ava allergic to penicillin?");
    await vi.advanceTimersByTimeAsync(2000);
    expect(phases(deltas)).toContain("done");
  });

  it("mock.needs_you.skips_settled", async () => {
    const engine = createMockEngine({ now: NOW });
    const which = (await engine.listNeedsYou()).find((e) => e.type === "which");
    expect(which).toBeDefined();
    if (which?.type !== "which") return;
    await engine.resolveWhich(which.conflictId, which.options[0]?.memoryId as string);
    const after = await engine.listNeedsYou();
    expect(after.some((e) => e.type === "which" && e.conflictId === which.conflictId)).toBe(false);
  });

  it("mock.new_item_emits", async () => {
    const engine = createMockEngine({ now: NOW });
    const seen: ItemView[] = [];
    engine.on("item-changed", (item) => seen.push(item));
    const kept = await engine.keepText("Ask Sam about Saturday", "quickCapture");
    expect(seen).toHaveLength(1);
    expect(seen[0]?.id).toBe(kept.type === "new" ? kept.item.id : "");
    expect(seen[0]?.status).toBe("kept");
  });
});

describe("MockEngine spoken page", () => {
  it("reads today's page from the recording, with the real section starts", async () => {
    const engine = createMockEngine({ now: NOW });
    const spoken = await engine.speakPage("2026-11-13");
    expect(spoken.sections.map((s) => s.kind)).toEqual([
      "opening",
      "needsYou",
      "changed",
      "letGo",
      "question",
    ]);
    expect(spoken.sections[0]?.startMs).toBe(0);
    const starts = spoken.sections.map((s) => s.startMs);
    expect(starts).toEqual([...starts].sort((a, b) => a - b));
    expect(new Set(starts).size).toBe(starts.length);
    expect(starts.at(-1) as number).toBeLessThan(spoken.durationMs);
    // The recording is far longer than the four-second placeholder parts.
    expect(spoken.durationMs).toBeGreaterThan(30_000);
  });

  it("recorded exactly the words the real script says for today's page", async () => {
    const engine = createMockEngine({ now: NOW });
    const page = await engine.getEdition("2026-11-13", "audio");
    const script = audioScript(page);
    expect(recorded.sections.map((s) => ({ kind: s.kind, text: s.text }))).toEqual(script);
  });

  it("keeps the recording small and its length in step with the file", () => {
    const file = resolve(process.cwd(), "src/engine/mock/assets/spoken-today.ogg");
    expect(statSync(file).size).toBeLessThan(300_000);
    expect(recorded.voice).toBe("bf_emma");
    const gaps = recorded.sections.map((s, i, all) => {
      const next = all[i + 1]?.startMs ?? recorded.durationMs;
      return next - s.startMs;
    });
    expect(gaps.every((g) => g > recorded.gapMs)).toBe(true);
  });

  it("leaves a page it has no recording of silent, with parts in order", async () => {
    const engine = createMockEngine({ now: NOW });
    const spoken = await engine.speakPage("2026-11-12");
    const starts = spoken.sections.map((s) => s.startMs);
    expect(starts).toEqual([...starts].sort((a, b) => a - b));
    expect(starts.at(-1) as number).toBeLessThan(spoken.durationMs);
  });
});

describe("MockEngine Away from home", () => {
  it("is off on a fresh start, and turning it on needs the download first", async () => {
    const engine = createMockEngine({ now: NOW, remote: { auto: false } });
    const view = await engine.getRemote();
    expect(view.state).toEqual({ type: "off" });
    expect(view.connection).toBe("simple");
    expect(view.autoOff).toBe("unused30Days");
    expect(view.devices).toEqual([]);
    await expect(engine.startRemote("simple", false)).rejects.toMatchObject({
      code: "needsDownload",
    });
    await expect(engine.beginPairing()).rejects.toMatchObject({ code: "unavailable" });
  });
});
