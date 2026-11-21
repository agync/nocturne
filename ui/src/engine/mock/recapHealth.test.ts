import { describe, expect, it } from "vitest";
import cases from "../../../../crates/engine/src/phones/noticing/cases.json";
import type {
  FeelingPart,
  ItemView,
  Noticed,
  NoticedTopic,
  ReadingRow,
  Recap,
  Receipt,
} from "../bindings";
import { PhonesSim } from "./phones";
import { type Body, type Counted, noticedOf, noticeRules, type Topic } from "./recapHealth";

// The table the engine's own noticing tests read, so the mock cannot drift from the rules.

interface Case {
  name: string;
  topics: Array<{ type: string; name?: string; part?: string; days: number[] }>;
  sleep: Record<string, number>;
  steps: Record<string, number>;
  workouts: number[];
  expect: Array<{
    measure: string;
    topic: string;
    days: number;
    held: number;
    otherDays: number;
    difference: number;
    more: boolean;
    body: number;
    journal: number;
  }>;
}

const day = (n: number | string): string => `2026-11-${String(n).padStart(2, "0")}`;

const receipt = (id: string): Receipt => ({
  itemId: id,
  kind: "reading",
  source: "phone",
  label: "x",
  keptAt: "2026-11-10T12:00:00+00:00",
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: null,
});

const counted = (prefix: string, values: Record<string, number>): Map<string, Counted> =>
  new Map(
    Object.entries(values).map(([d, value]) => [
      day(d),
      { value, receipt: receipt(`${prefix}-${d}`) },
    ]),
  );

const topicOf = (t: Case["topics"][number]): Topic => {
  const label = t.name ?? t.part ?? "";
  const kind: NoticedTopic =
    t.type === "name"
      ? {
          type: "name",
          with: {
            entityId: label,
            name: label,
            kind: "person",
            role: null,
            new: false,
            itemIds: [],
          },
        }
      : t.type === "felt"
        ? { type: "felt", part: (t.part ?? "settled") as FeelingPart }
        : { type: "good" };
  return {
    kind,
    days: new Map(t.days.map((n) => [day(n), [receipt(`${label}-entry-${n}`)]])),
  };
};

const summary = (n: Noticed) => ({
  measure: n.measure,
  topic:
    n.topic.type === "name"
      ? `name:${n.topic.with.name}`
      : n.topic.type === "felt"
        ? `felt:${n.topic.part}`
        : "good",
  days: n.days,
  held: n.held,
  otherDays: n.otherDays,
  difference: n.difference,
  more: n.more,
  body: n.body.length,
  journal: n.journal.length,
});

describe("the mock's noticing rules", () => {
  it.each(cases.cases as unknown as Case[])("$name", (c) => {
    const workouts = new Map<string, Receipt[]>();
    for (const n of c.workouts) workouts.set(day(n), [receipt(`workout-${n}`)]);
    const body: Body = {
      sleep: counted("sleep", c.sleep),
      steps: counted("steps", c.steps),
      workouts,
    };
    expect(noticeRules(c.topics.map(topicOf), body).map(summary)).toEqual(c.expect);
  });
});

describe("the mock's body", () => {
  const stepsRow = (id: string, d: string, total: number, phone: string): ReadingRow => ({
    itemId: id,
    day: d,
    start: `${d}T08:00:00.000Z`,
    end: null,
    value: { type: "steps", total },
    sourceDevice: phone,
    phone,
    removed: false,
  });
  const item = (id: string, d: string): ItemView =>
    ({ id, kind: "text", title: id, source: "journal", keptAt: `${d}T09:00:00.000Z` }) as ItemView;

  it("counts the most steps of two phones for a day, and only up to yesterday while it goes on", async () => {
    const rows: ReadingRow[] = [];
    for (let n = 9; n <= 15; n += 1) {
      const low = n >= 10 && n <= 12;
      rows.push(stepsRow(`a${n}`, day(n), low ? 6000 : 8000, "Pixel"));
      rows.push(stepsRow(`b${n}`, day(n), 100, "Watch"));
    }
    const sim = new PhonesSim({ phones: [], cameIn: {}, series: {}, readings: { steps: rows } });
    const recap = {
      period: { kind: "week", start: day(9), end: day(15) },
      people: [],
      checkins: [],
      good: [10, 11, 12].map((n) => ({ id: `g${n}`, day: day(n) })),
    } as unknown as Recap;
    const get = async (id: string) => item(id, day(id.slice(1)));
    const done = await noticedOf(sim, recap, day(16), get);
    expect(done.map(summary)).toEqual([
      {
        measure: "steps",
        topic: "good",
        days: 3,
        held: 3,
        otherDays: 4,
        difference: 2000,
        more: false,
        body: 3,
        journal: 3,
      },
    ]);
    expect(done[0]?.body.every((r) => r.itemId.startsWith("a"))).toBe(true);
    // On the 12th, the 11th is the last day counted: two days with the thing are left.
    expect(await noticedOf(sim, recap, day(12), get)).toEqual([]);
  });
});
