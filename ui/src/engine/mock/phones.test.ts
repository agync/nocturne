import { describe, expect, it } from "vitest";
import { createMockEngine } from "./engine";
import { withPhones } from "./phones";

const NOW = "2026-11-13T07:02:00Z";
const phones = () => withPhones(createMockEngine(), NOW);

describe("the mock's phones", () => {
  it("bring a calendar and health from Sam’s phone and nothing from the browser", async () => {
    const view = await phones().getPhoneSources();
    expect(view.phones.map((p) => [p.name, p.inBrowser])).toEqual([
      ["Sam’s phone", false],
      ["Ana’s phone", true],
    ]);
    expect(view.phones[0]?.calendar?.taking).toBe(true);
    expect(view.phones[0]?.health?.kinds).not.toContain("weight");
    expect(view.phones[1]?.calendar).toBeNull();
  });

  it("add today's calendar and last night to the page, and take them away when switched off", async () => {
    const engine = phones();
    const page = await engine.getPage();
    expect(page.calendarToday?.map((l) => l.title)).toEqual([
      "Dentist, check-up",
      "Leo, swimming",
      "Book group",
    ]);
    expect(page.lastNight?.asleepMin).toBe(400);
    await engine.setPhonePageOptions({ calendarToday: false, sleepLastNight: false });
    const quiet = await engine.getPage();
    expect(quiet.calendarToday).toBeUndefined();
    expect(quiet.lastNight).toBeUndefined();
  });

  it("add the calendar, workout and woke rows and the health block to the journal day", async () => {
    const engine = phones();
    const day = await engine.getJournalDay();
    expect(day.rows.map((r) => r.kind)).toEqual(
      expect.arrayContaining(["woke", "calendar", "workout"]),
    );
    expect(day.health?.night?.itemId).toBe("read-sleep-0");
    expect(day.health?.pressure).toHaveLength(1);
  });

  it("answer for a calendar entry's own page with its details", async () => {
    const item = await phones().getItem("cal-leo-swim");
    expect(item.kind).toBe("calendar");
    expect(await phones().getItem("read-sleep-0")).toMatchObject({ kind: "reading" });
  });

  it("lists what came in newest first, and pages it", async () => {
    const engine = phones();
    const first = await engine.listCameIn("pixel", "calendar", null, 2);
    expect(first.map((r) => r.title)).toEqual(["Leo, swimming", "Book group"]);
    const rest = await engine.listCameIn("pixel", "calendar", first[1]?.cameInAt ?? null, 10);
    expect(rest.length).toBeGreaterThan(0);
  });

  it("deletes what came in, and stops taking it when asked", async () => {
    const engine = phones();
    const preview = await engine.previewPhoneDelete({
      deviceId: "pixel",
      source: "calendar",
      kinds: null,
      before: null,
    });
    expect(preview.items).toBeGreaterThan(0);
    await engine.deletePhoneData(preview.token, true);
    expect(await engine.listCameIn("pixel", "calendar", null, 10)).toEqual([]);
    const view = await engine.getPhoneSources();
    expect(view.phones[0]?.calendar?.taking).toBe(false);
    expect((await engine.getPage()).calendarToday).toBeUndefined();
    await expect(engine.deletePhoneData(preview.token, false)).rejects.toMatchObject({
      code: "tokenExpired",
    });
  });

  it("deletes health readings by kind and by date", async () => {
    const engine = phones();
    const scope = {
      deviceId: "pixel",
      source: "health" as const,
      kinds: ["sleep" as const],
      before: null,
    };
    const preview = await engine.previewPhoneDelete(scope);
    expect(preview.items).toBeGreaterThan(10);
    await engine.deletePhoneData(preview.token, false);
    expect(await engine.listReadings("sleep", "2026-01-01", "2026-12-31")).toEqual([]);
    expect(await engine.listReadings("steps", "2026-01-01", "2026-12-31")).not.toEqual([]);
  });

  it("show the phones' names in Away from home", async () => {
    const engine = withPhones(createMockEngine({ remote: { auto: false, scenario: "on" } }), NOW);
    expect((await engine.getRemote()).devices.map((d) => d.name)).toEqual([
      "Sam’s phone",
      "Ana’s phone",
    ]);
  });
});
