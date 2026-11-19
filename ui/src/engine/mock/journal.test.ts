import { describe, expect, it } from "vitest";
import type { Period } from "../bindings";
import { createMockEngine, type MockEngine } from "./engine";
import type { JournalMode } from "./seed/journal";

const NOW = new Date("2026-11-19T21:20:00Z");

function engineIn(mode: JournalMode = "day"): MockEngine {
  return createMockEngine({ now: NOW, journal: { mode, delayMs: 0 } });
}

const dayOf = (engine: MockEngine) => engine.journal.today;
const dayPeriod = (day: string): Period => ({ kind: "day", start: day, end: day });

describe("the mock journal's day", () => {
  it("holds the mockups' Thursday: the entry and the timeline in order", async () => {
    const engine = engineIn();
    const day = await engine.getJournalDay();
    expect(day.today).toBe(true);
    expect(day.firstUse).toBe(false);
    expect(day.entry.map((b) => b.item.kind)).toEqual(["text", "audio", "image", "text"]);
    const times = day.rows.map(
      (r) => new Date(r.item.keptAt).getHours() * 60 + new Date(r.item.keptAt).getMinutes(),
    );
    expect(times).toEqual([...times].sort((a, b) => a - b));
    expect(day.rows.map((r) => r.kind)).toEqual([
      "note",
      "checkin",
      "elsewhere",
      "note",
      "photo",
      "checkin",
      "good",
      "good",
      "good",
      "entry",
    ]);
    expect(day.rows[3]?.feeling?.feeling.words).toEqual(["pleased"]);
    expect(day.checkins.map((c) => c.feeling.words.join(", "))).toEqual([
      "tired",
      "pleased",
      "relieved, content",
    ]);
    expect(day.good).toHaveLength(3);
    expect(day.people.map((p) => [p.name, p.new])).toContainEqual(["Priya", true]);
    expect(day.people.map((p) => [p.name, p.new])).toContainEqual(["the climbing wall", true]);
    expect(day.reflection?.firstSentence).toBe("A slow start, then lunch with Priya at last.");
    expect(day.sameName).toHaveLength(1);
  });

  it("starts the first-use card, an empty day, and a day still being read", async () => {
    const first = await engineIn("first").getJournalDay();
    expect(first.firstUse).toBe(true);
    expect(first.rows).toEqual([]);
    const empty = await engineIn("empty").getJournalDay();
    expect(empty.firstUse).toBe(false);
    expect(empty.entry).toEqual([]);
    const partial = await engineIn("partial").getJournalDay();
    expect(partial.reading).toBe(2);
    expect(partial.people).toEqual([]);
  });

  it("dismisses the first-use card", async () => {
    const engine = engineIn("first");
    await engine.dismissJournalIntro();
    expect((await engine.getJournalDay()).firstUse).toBe(false);
  });

  it("holds lighter days before today, and lists them oldest first", async () => {
    const engine = engineIn();
    const days = await engine.listJournalDays();
    expect(days).toEqual([...days].sort());
    expect(days).toContain(dayOf(engine));
    const wed = await engine.getJournalDay("2026-11-18");
    expect(wed.today).toBe(false);
    expect(wed.rows.length).toBeGreaterThan(0);
    expect(wed.rows.length).toBeLessThan((await engine.getJournalDay()).rows.length);
  });

  it("says the engine is getting ready only in that state", async () => {
    expect((await engineIn("notready").getStatus()).state).toBe("gettingReady");
    expect((await engineIn().getStatus()).state).not.toBe("gettingReady");
  });

  it("answers an item's receipt with the journal item itself", async () => {
    const engine = engineIn();
    const item = await engine.getItem("j-ci-tired");
    expect(item.kind).toBe("checkin");
  });
});

describe("writing", () => {
  it("saves a new paragraph, changes one in place while unread, and keeps a version once read", async () => {
    const engine = engineIn("empty");
    const day = dayOf(engine);
    const first = await engine.saveEntryText(day, null, 0, "Rain again.");
    expect(first?.block).toBe(0);
    const again = await engine.saveEntryText(
      day,
      first?.item.id ?? null,
      0,
      "Rain again, and cold.",
    );
    expect(again?.item.id).toBe(first?.item.id);
    expect(again?.earlier).toEqual([]);
    await engine.readEntryNow(day);
    const changed = await engine.saveEntryText(
      day,
      first?.item.id ?? null,
      0,
      "Rain again, and warm.",
    );
    expect(changed?.item.id).not.toBe(first?.item.id);
    expect(changed?.earlier.map((e) => e.text)).toEqual(["Rain again, and cold."]);
    expect(changed?.changedAt).not.toBeNull();
    expect((await engine.getJournalDay(day)).entry).toHaveLength(1);
  });

  it("removes an unread paragraph that is emptied, and refuses nothing new for empty text", async () => {
    const engine = engineIn("empty");
    const day = dayOf(engine);
    expect(await engine.saveEntryText(day, null, 0, "  ")).toBeNull();
    const made = await engine.saveEntryText(day, null, 0, "Hello");
    expect(await engine.saveEntryText(day, made?.item.id ?? null, 0, "")).toBeNull();
    expect((await engine.getJournalDay(day)).entry).toEqual([]);
  });

  it("reads the entry now: nothing is left reading", async () => {
    const engine = engineIn("partial");
    const day = dayOf(engine);
    await engine.readEntryNow(day);
    expect((await engine.getJournalDay(day)).reading).toBe(0);
  });

  it("describes a photo as its alternative text", async () => {
    const engine = engineIn();
    await engine.describePhoto("j-block-photo", "Three tomatoes, one red");
    const block = (await engine.getJournalDay()).entry.find((b) => b.item.id === "j-block-photo");
    expect(block?.alt).toBe("Three tomatoes, one red");
  });

  it("places a photo kept elsewhere into the entry", async () => {
    const engine = engineIn("empty");
    const kept = await engine.keepBytes(new Uint8Array(20), "image/jpeg", "journal");
    if (kept.type !== "new") throw new Error("expected a new item");
    const placed = await engine.placeInEntry(kept.item.id, dayOf(engine), 0);
    expect(placed.item.id).toBe(kept.item.id);
    expect((await engine.getJournalDay()).entry).toHaveLength(1);
  });
});

describe("keeping", () => {
  it("keeps a quick note with a feeling and puts it on the timeline", async () => {
    const engine = engineIn("empty");
    const feeling = {
      part: "litUp" as const,
      point: { pleasant: 0.4, energy: 0.3 },
      words: ["pleased"],
    };
    const kept = await engine.keepNote("Lunch with Priya, finally", "quickCapture", feeling, false);
    expect(kept.type).toBe("new");
    const day = await engine.getJournalDay();
    expect(day.rows).toHaveLength(1);
    expect(day.rows[0]?.feeling?.feeling.words).toEqual(["pleased"]);
    expect(day.checkins).toHaveLength(1);
  });

  it("takes back a note with the feeling kept beside it", async () => {
    const engine = engineIn("empty");
    const kept = await engine.keepNote(
      "Lunch with Priya, finally",
      "quickCapture",
      { part: "litUp", point: { pleasant: 0.4, energy: 0.3 }, words: ["pleased"] },
      false,
    );
    await engine.unkeep(kept.item.id);
    const day = await engine.getJournalDay();
    expect(day.rows).toHaveLength(0);
    expect(day.checkins).toHaveLength(0);
  });

  it("keeps a note marked as a good thing as a good thing", async () => {
    const engine = engineIn("empty");
    await engine.keepNote("Sam fixed the gate", "phone", null, true);
    const day = await engine.getJournalDay();
    expect(day.good.map((g) => g.text)).toEqual(["Sam fixed the gate"]);
    expect(day.rows.map((r) => r.kind)).toEqual(["good"]);
  });

  it("keeps a check-in, and a change keeps the day of the one it replaces", async () => {
    const engine = engineIn();
    const day = dayOf(engine);
    const before = (await engine.getJournalDay(day)).checkins.length;
    const feeling = {
      part: "settled" as const,
      point: { pleasant: 0.6, energy: -0.5 },
      words: ["calm"],
    };
    const saved = await engine.keepCheckIn({ feeling, line: "Quiet.", urge: null, replaces: null });
    expect((await engine.getJournalDay(day)).checkins).toHaveLength(before + 1);
    const changed = await engine.keepCheckIn({
      feeling: { ...feeling, words: ["peaceful"] },
      line: "Quiet.",
      urge: null,
      replaces: saved.id,
    });
    const after = (await engine.getJournalDay(day)).checkins;
    expect(after).toHaveLength(before + 1);
    expect(after.map((c) => c.id)).toContain(changed.id);
    expect(after.map((c) => c.id)).not.toContain(saved.id);
  });

  it("notes an idea as tried, once", async () => {
    const engine = engineIn();
    await engine.markTried("j-ci-tired", "journal.help.idea.move.walk");
    await engine.markTried("j-ci-tired", "journal.help.idea.move.walk");
    const day = await engine.getJournalDay();
    expect(day.checkins[0]?.tried).toEqual(["journal.help.idea.move.walk"]);
  });

  it("keeps up to three good things a day, never a fourth", async () => {
    const engine = engineIn("empty");
    const day = dayOf(engine);
    for (const text of ["One", "Two", "Three"]) await engine.keepGoodThing(day, text, null);
    await expect(engine.keepGoodThing(day, "Four", null)).rejects.toThrow();
    expect((await engine.getJournalDay(day)).good).toHaveLength(3);
  });

  it("offers the four words used most in a part, then the first of the list", async () => {
    const engine = engineIn("empty");
    expect(await engine.topFeelingWords("woundUp")).toEqual([
      "worried",
      "stressed",
      "uneasy",
      "restless",
    ]);
    expect(await engine.topFeelingWords("litUp")).toEqual([
      "proud",
      "pleased",
      "cheerful",
      "curious",
    ]);
    const populated = engineIn();
    const words = await populated.topFeelingWords("settled");
    expect(words).toHaveLength(4);
    expect(words[0]).toBe("content");
  });

  it("leaves an item out of reflections, and takes it back", async () => {
    const engine = engineIn();
    await engine.setLeftOut("j-note-leo", true);
    const row = (await engine.getJournalDay()).rows.find((r) => r.item.id === "j-note-leo");
    expect(row?.leftOut).toBe(true);
    await engine.setLeftOut("j-note-leo", false);
    const back = (await engine.getJournalDay()).rows.find((r) => r.item.id === "j-note-leo");
    expect(back?.leftOut).toBe(false);
  });

  it("moves a note to another day", async () => {
    const engine = engineIn();
    await engine.moveToDay("j-note-leo", "2026-11-18");
    expect((await engine.getJournalDay()).rows.some((r) => r.item.id === "j-note-leo")).toBe(false);
    expect(
      (await engine.getJournalDay("2026-11-18")).rows.some((r) => r.item.id === "j-note-leo"),
    ).toBe(true);
  });
});

describe("people and places", () => {
  const mentionIn = async (engine: MockEngine, name: string) => {
    const day = await engine.getJournalDay();
    const all = day.entry.flatMap((b) => b.mentions);
    const found = all.find((m) => m.name === name);
    if (!found) throw new Error(`No mention of ${name}`);
    return found;
  };

  it("confirms with a role, removes, restores and refiles a mention", async () => {
    const engine = engineIn();
    const priya = await mentionIn(engine, "Priya");
    const confirmed = await engine.confirmMention(priya.id, "a friend from climbing");
    expect(confirmed.state).toBe("confirmed");
    expect(confirmed.role).toBe("a friend from climbing");
    expect((await engine.removeMention(priya.id)).state).toBe("removed");
    const inEntry = (await engine.getJournalDay()).entry.flatMap((b) => b.mentions);
    expect(inEntry.find((m) => m.id === priya.id)?.state).toBe("removed");
    expect((await engine.restoreMention(priya.id)).state).toBe("found");
    const wall = await mentionIn(engine, "the climbing wall");
    const moved = await engine.refileMention(wall.id, {
      type: "new",
      name: "Boulder Barn",
      kind: "place",
    });
    expect(moved.name).toBe("Boulder Barn");
    expect((await engine.getJournalDay()).people.map((p) => p.name)).toContain("Boulder Barn");
  });

  it("answers Which Sam is this: files it, and the question leaves Needs you", async () => {
    const engine = engineIn();
    expect((await engine.listNeedsYou()).filter((e) => e.type === "sameName")).toHaveLength(1);
    const done = await engine.answerSameName("sn-sam", {
      type: "entity",
      entityId: "ent-sam-work",
    });
    expect(done).toEqual({ name: "Sam", role: "from work" });
    expect((await engine.listNeedsYou()).some((e) => e.type === "sameName")).toBe(false);
    expect((await engine.getJournalDay()).sameName).toEqual([]);
    expect(await engine.entityJournalDays("ent-sam-work")).toContain(dayOf(engine));
  });

  it("leaves it unfiled when told to", async () => {
    const engine = engineIn();
    const done = await engine.answerSameName("sn-sam", { type: "leave" });
    expect(done.role).toBeNull();
    expect(await engine.entityJournalDays("ent-sam-work")).toEqual([]);
  });

  it("lists the days a person appears in, newest first", async () => {
    const days = await engineIn().entityJournalDays("ent-gran");
    expect(days.length).toBeGreaterThan(2);
    expect(days).toEqual([...days].sort().reverse());
  });
});

describe("looking back", () => {
  it("holds today's reflection so far, and writes it again on demand", async () => {
    const engine = engineIn();
    const period = dayPeriod(dayOf(engine));
    const saved = await engine.getReflection(period);
    expect(saved.type).toBe("written");
    if (saved.type !== "written") return;
    expect(saved.reflection.soFar).toBe(true);
    expect(saved.reflection.fromCount).toBe(6);
    const wrote = await engine.reflect(period);
    expect(wrote.type).toBe("written");
    if (wrote.type !== "written") return;
    expect(wrote.reflection.written).toBe("onDemand");
    expect(wrote.reflection.theDay.length).toBeGreaterThan(2);
    for (const line of wrote.reflection.theDay) expect(line.receipts.length).toBeGreaterThan(0);
    expect(wrote.reflection.felt).toHaveLength(3);
    expect(wrote.reflection.stillOpen).toHaveLength(1);
  });

  it("hides lines that cite what is left out, at once", async () => {
    const engine = engineIn();
    const period = dayPeriod(dayOf(engine));
    await engine.reflect(period);
    await engine.setLeftOut("j-note-leo", true);
    const view = await engine.getReflection(period);
    if (view.type !== "written") throw new Error("expected a written day");
    expect(
      view.reflection.theDay.some((l) => l.receipts.some((r) => r.itemId === "j-note-leo")),
    ).toBe(false);
    await engine.setLeftOut("j-note-leo", false);
    const back = await engine.getReflection(period);
    if (back.type !== "written") throw new Error("expected a written day");
    expect(
      back.reflection.theDay.some((l) => l.receipts.some((r) => r.itemId === "j-note-leo")),
    ).toBe(true);
  });

  it("deletes only the writing, and says nothing yet afterwards", async () => {
    const engine = engineIn();
    const period = dayPeriod(dayOf(engine));
    await engine.deleteReflection(period);
    const after = await engine.getReflection(period);
    expect(after.type).toBe("notYet");
    expect((await engine.getJournalDay()).entry.length).toBeGreaterThan(0);
    expect((await engine.getJournalDay()).reflection).toBeNull();
  });

  it("says there is nothing to look back on for an empty day", async () => {
    const engine = engineIn("empty");
    expect((await engine.getReflection(dayPeriod(dayOf(engine)))).type).toBe("nothing");
  });

  it("stops writing when asked", async () => {
    const engine = createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 20 } });
    const period = dayPeriod("2026-11-18");
    await engine.deleteReflection(period);
    const pending = engine.reflect(period);
    await engine.stopReflecting(period);
    expect((await pending).type).toBe("notYet");
  });

  it("holds a week, a month and a year", async () => {
    const engine = engineIn();
    const week = await engine.getReflection({
      kind: "week",
      start: "2026-11-09",
      end: "2026-11-15",
    });
    if (week.type !== "recap") throw new Error("expected a week recap");
    const recap = week.recap;
    expect(recap.highlights.map((h) => h.day)).toEqual([
      "2026-11-10",
      "2026-11-11",
      "2026-11-14",
      "2026-11-15",
    ]);
    expect(recap.checkins).toHaveLength(8);
    expect(recap.words.map((w) => [w.word, w.checkinIds.length])).toEqual([
      ["tired", 3],
      ["content", 2],
      ["stressed", 2],
      ["proud", 1],
    ]);
    expect(recap.people.map((p) => [p.with.name, p.days.length]).slice(0, 4)).toEqual([
      ["Leo", 7],
      ["Gran", 4],
      ["Sam", 3],
      ["the allotment", 2],
    ]);
    expect(recap.together[0]?.count).toBe(3);
    expect(recap.stillOpen).toHaveLength(1);
    const month = await engine.getReflection({
      kind: "month",
      start: "2026-11-01",
      end: "2026-11-30",
    });
    expect(month.type).toBe("recap");
    const year = await engine.getReflection({
      kind: "year",
      start: "2025-01-01",
      end: "2025-12-31",
    });
    if (year.type !== "recap") throw new Error("expected a year recap");
    expect(year.recap.stillOpen).toEqual([]);
    expect(year.recap.highlights.length).toBeGreaterThanOrEqual(6);
  });

  it("lists weeks, months and years that hold anything, newest first", async () => {
    const engine = engineIn();
    const weeks = await engine.listPeriods("week");
    expect(weeks[0]?.period.start).toBe("2026-11-16");
    expect(weeks.map((w) => w.period.start)).toEqual(
      [...weeks.map((w) => w.period.start)].sort().reverse(),
    );
    expect((await engine.listPeriods("year")).map((y) => y.period.start.slice(0, 4))).toEqual([
      "2026",
      "2025",
    ]);
    expect(weeks.find((w) => w.period.start === "2026-11-09")?.written).toBe(true);
  });
});

describe("what the mock answers as the engine does", () => {
  const today = (engine: MockEngine) => dayPeriod(dayOf(engine));
  const WEEK: Period = { kind: "week", start: "2026-11-09", end: "2026-11-15" };

  it("gives a saved reflection before any blocked state", async () => {
    const engine = engineIn("glance");
    expect((await engine.getReflection(today(engine))).type).toBe("glance");
    expect((await engine.getReflection(dayPeriod("2026-11-18"))).type).toBe("written");
    expect((await engine.getReflection(WEEK)).type).toBe("recap");
  });

  it("says nothing was kept before it says it is not ready", async () => {
    const engine = engineIn("notready");
    expect((await engine.getReflection(dayPeriod("2026-10-01"))).type).toBe("nothing");
    expect((await engine.getReflection(today(engine))).type).toBe("glance");
  });

  it("leaves things kept from elsewhere out of the names", async () => {
    const engine = engineIn();
    const data = (
      engine.journal as unknown as {
        data: { recs: Array<{ t: string; row?: string; mentions?: unknown[] }> };
      }
    ).data;
    const made = (await engine.getJournalDay()).people.find((p) => p.name === "Sam");
    const mention = (await engine.getJournalDay()).entry
      .flatMap((b) => b.mentions)
      .find((m) => m.name === "Sam");
    if (!made || !mention) throw new Error("expected Sam");
    const form = data.recs.find((r) => r.row === "elsewhere");
    form?.mentions?.push({
      ...mention,
      id: "men-form",
      entityId: "ent-form-only",
      name: "The Head Teacher",
      itemId: "j-form",
    });
    const day = await engine.getJournalDay();
    expect(day.people.map((p) => p.name)).not.toContain("The Head Teacher");
    const view = await engine.reflect(today(engine));
    if (view.type !== "written") throw new Error("expected a written day");
    expect(view.reflection.people.map((p) => p.name)).not.toContain("The Head Teacher");
  });

  it("hides recap lines that cite what is left out", async () => {
    const engine = engineIn();
    const before = await engine.getReflection(WEEK);
    if (before.type !== "recap") throw new Error("expected a recap");
    const cited = before.recap.highlights[0]?.receipts[0]?.itemId;
    if (!cited) throw new Error("expected a highlight with a receipt");
    await engine.setLeftOut(cited, true);
    const after = await engine.getReflection(WEEK);
    if (after.type !== "recap") throw new Error("expected a recap");
    const cites = (r: { receipts: Array<{ itemId: string }> }) =>
      r.receipts.some((x) => x.itemId === cited);
    expect(after.recap.highlights.some(cites)).toBe(false);
    expect(after.recap.together.some(cites)).toBe(false);
    expect(after.recap.changed.some(cites)).toBe(false);
    expect(after.recap.highlights.length).toBeLessThan(before.recap.highlights.length);
    await engine.setLeftOut(cited, false);
    const back = await engine.getReflection(WEEK);
    if (back.type !== "recap") throw new Error("expected a recap");
    expect(back.recap.highlights).toHaveLength(before.recap.highlights.length);
  });

  it("cites something for what changed", async () => {
    const engine = engineIn();
    const view = await engine.getReflection(WEEK);
    if (view.type !== "recap") throw new Error("expected a recap");
    expect(view.recap.changed.length).toBeGreaterThan(0);
    for (const line of view.recap.changed) expect(line.receipts.length).toBeGreaterThan(0);
  });

  it("works out who a part of the field went with from the days", async () => {
    const engine = engineIn();
    const view = await engine.getReflection(WEEK);
    if (view.type !== "recap") throw new Error("expected a recap");
    const { recap } = view;
    expect(recap.together.length).toBeGreaterThan(0);
    for (const line of recap.together) {
      const days = recap.people.find((p) => p.with.entityId === line.with.entityId)?.days ?? [];
      expect(line.total).toBe(days.length);
      expect(line.count).toBe(line.receipts.length);
      expect(line.count).toBeGreaterThanOrEqual(3);
      for (const r of line.receipts) {
        const c = recap.checkins.find((x) => x.id === r.itemId);
        expect(c?.feeling.part).toBe(line.part);
        expect(days).toContain(c?.day);
      }
    }
  });
});

describe("the other states of looking back", () => {
  const today = (engine: MockEngine) => dayPeriod(dayOf(engine));

  it("is a glance while the engine is getting ready", async () => {
    const engine = engineIn("notready");
    const view = await engine.getReflection(today(engine));
    expect(view.type).toBe("glance");
    if (view.type === "glance") expect(view.reason).toBe("notReady");
    expect((await engine.reflect(today(engine))).type).toBe("glance");
  });

  it("is a glance on a computer that cannot write it", async () => {
    const engine = engineIn("glance");
    const view = await engine.getReflection(today(engine));
    if (view.type !== "glance") throw new Error("expected a glance");
    expect(view.reason).toBe("notOnThisComputer");
    expect(view.glance.kept.length).toBeGreaterThan(0);
  });

  it("answers writing until it is stopped", async () => {
    const engine = engineIn("writing");
    expect((await engine.getReflection(today(engine))).type).toBe("writing");
    await engine.stopReflecting(today(engine));
    expect((await engine.getReflection(today(engine))).type).not.toBe("writing");
  });
});

describe("the address picks the mock's clock and journal", () => {
  const visit = (query: string) => window.history.replaceState({}, "", `/${query}`);
  const leave = () => window.history.replaceState({}, "", "/");

  it("reads ?mockNow and ?mockJournal", async () => {
    visit("?mockNow=2026-11-19T21:20:00Z&mockJournal=first");
    try {
      const engine = createMockEngine();
      const day = await engine.getJournalDay();
      expect(day.day).toBe("2026-11-19");
      expect(day.firstUse).toBe(true);
    } finally {
      leave();
    }
  });

  it("ignores a state it does not know", async () => {
    visit("?mockNow=2026-11-19T21:20:00Z&mockJournal=nonsense");
    try {
      const day = await createMockEngine().getJournalDay();
      expect(day.rows.length).toBeGreaterThan(0);
    } finally {
      leave();
    }
  });
});
