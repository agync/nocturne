import { addDays } from "../../copy/format";
import type {
  FeelingPart,
  HealthSeries,
  ItemView,
  Noticed,
  NoticedTopic,
  Period,
  ReadingKind,
  ReadingRow,
  Recap,
  RecapHealth,
  Receipt,
} from "../bindings";
import type { PhonesSim } from "./phones";
import { journalDayOf } from "./seed/journal";

const noon = (day: string): string => new Date(`${day}T12:00:00`).toISOString();

const readingReceipt = (row: ReadingRow, label: string): Receipt => ({
  itemId: row.itemId,
  kind: "reading",
  source: "phone",
  label,
  keptAt: noon(row.day),
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: row.phone,
});

const asleepMinutes = (row: ReadingRow): number =>
  row.value.type === "sleep"
    ? row.value.asleep.reduce((sum, p) => sum + (Date.parse(p.to) - Date.parse(p.from)) / 60_000, 0)
    : 0;

const live = (sim: PhonesSim, kind: ReadingKind, from: string, to: string): ReadingRow[] =>
  sim.readings(kind, from, to).filter((r) => !r.removed);

/** One reading for each night: the longest of the day, as the engine chooses it. */
function longestNights(rows: ReadingRow[]): ReadingRow[] {
  const best = new Map<string, ReadingRow>();
  for (const row of rows) {
    const have = best.get(row.day);
    if (!have || asleepMinutes(row) > asleepMinutes(have)) best.set(row.day, row);
  }
  return [...best.values()];
}

/** "From Health" for a period, from the mock's readings: nothing the design leaves out of recaps. */
export function recapHealthOf(sim: PhonesSim, period: Period): RecapHealth | null {
  const { start: from, end: to } = period;
  const series = (kind: ReadingKind): HealthSeries | null => {
    const s = sim.series(kind, from, to);
    return s.points.length > 0 ? s : null;
  };
  const sleep = series("sleep");
  const steps = series("steps");
  const resting = series("restingHeartRate");
  const workouts = live(sim, "workout", from, to)
    .sort((a, b) => a.day.localeCompare(b.day))
    .map((r) => readingReceipt(r, r.value.type === "workout" ? r.value.kind : "Workout"));
  if (!sleep && !steps && !resting && workouts.length === 0) return null;
  return {
    nights: period.kind === "year" ? [] : longestNights(live(sim, "sleep", from, to)),
    sleep,
    steps,
    resting,
    workouts,
    noticed: [],
  };
}

const receiptOf = (item: ItemView): Receipt => ({
  itemId: item.id,
  kind: item.kind,
  source: item.source,
  label: item.title,
  keptAt: item.keptAt,
  durationMs: item.durationMs,
  span: null,
  unconfirmed: false,
  fromDevice: item.fromDevice,
});

const MIN_DAYS = 3;
const LEAST: Record<"sleep" | "steps", number> = { sleep: 30, steps: 2000 };
const MOST = 3;

export interface Topic {
  kind: NoticedTopic;
  days: Map<string, Receipt[]>;
}

export interface Counted {
  value: number;
  receipt: Receipt;
}

export interface Body {
  sleep: Map<string, Counted>;
  steps: Map<string, Counted>;
  workouts: Map<string, Receipt[]>;
}

interface Found {
  line: Noticed;
  strength: number;
}

const PART_LABEL: Record<FeelingPart, string> = {
  woundUp: "WoundUp",
  litUp: "LitUp",
  low: "Low",
  settled: "Settled",
};

const labelOf = (kind: NoticedTopic): string =>
  kind.type === "name" ? kind.with.name : kind.type === "felt" ? PART_LABEL[kind.part] : "";

const sum = (xs: number[]): number => xs.reduce((a, b) => a + b, 0);

/** One topic against sleep or steps: the days with the topic against the others. */
function against(
  topic: Topic,
  measure: "sleep" | "steps",
  values: Map<string, Counted>,
): Found | null {
  const least = LEAST[measure];
  const withDays = [...values].filter(([d]) => topic.days.has(d));
  const without = [...values].filter(([d]) => !topic.days.has(d)).map(([, c]) => c.value);
  if (withDays.length < MIN_DAYS || without.length < MIN_DAYS) return null;
  const withValues = withDays.map(([, c]) => c.value);
  const [nw, no] = [withValues.length, without.length];
  // The gap between the two averages as a fraction, so a threshold edge is exact.
  const top = sum(withValues) * no - sum(without) * nw;
  const scale = nw * no;
  if (top === 0 || Math.abs(top) < least * scale) return null;
  const more = top > 0;
  const held = withValues.filter((v) => {
    const across = v * no - sum(without);
    return more ? across > 0 : across < 0;
  }).length;
  if (held < MIN_DAYS) return null;
  return {
    strength: Math.floor((Math.abs(top) * 1000) / (least * scale)),
    line: {
      measure,
      topic: topic.kind,
      days: topic.days.size,
      held,
      otherDays: no,
      difference: Math.max(1, Math.round(Math.abs(top) / scale)),
      more,
      body: withDays.map(([, c]) => c.receipt),
      journal: withDays.flatMap(([d]) => topic.days.get(d) ?? []),
    },
  };
}

/** Workout days against the others, for a part of the field felt on them. */
function onWorkoutDays(topic: Topic, body: Body): Found | null {
  const known = new Set([...body.sleep.keys(), ...body.steps.keys(), ...body.workouts.keys()]);
  const active = [...body.workouts.keys()].sort();
  const quiet = [...known].filter((d) => !body.workouts.has(d));
  if (active.length < MIN_DAYS || quiet.length < MIN_DAYS) return null;
  const hits = active.filter((d) => topic.days.has(d));
  const quietHits = quiet.filter((d) => topic.days.has(d)).length;
  if (hits.length < MIN_DAYS) return null;
  const [a, q] = [active.length, quiet.length];
  const gap = hits.length * q - quietHits * a;
  if (gap * 4 < a * q) return null;
  return {
    strength: Math.floor((gap * 1000) / (a * q)),
    line: {
      measure: "workout",
      topic: topic.kind,
      days: a,
      held: hits.length,
      otherDays: q,
      difference: 0,
      more: true,
      body: active.flatMap((d) => body.workouts.get(d) ?? []),
      journal: hits.flatMap((d) => topic.days.get(d) ?? []),
    },
  };
}

/** The rules of what a recap may notice, as the engine has them (`phones/noticing.rs`). */
export function noticeRules(topics: readonly Topic[], body: Body): Noticed[] {
  const found: Found[] = [];
  for (const topic of topics) {
    // The later of two equal effects wins, as the engine's maximum does.
    let best: Found | null = null;
    for (const f of [against(topic, "sleep", body.sleep), against(topic, "steps", body.steps)]) {
      if (f && (!best || f.strength >= best.strength)) best = f;
    }
    if (best) found.push(best);
    if (topic.kind.type === "felt") {
      const f = onWorkoutDays(topic, body);
      if (f) found.push(f);
    }
  }
  const order = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  return found
    .sort(
      (a, b) =>
        b.line.days - a.line.days ||
        b.strength - a.strength ||
        order(labelOf(a.line.topic), labelOf(b.line.topic)),
    )
    .slice(0, MOST)
    .map((f) => f.line);
}

/** What a recap notices over the mock's readings and the recap's own journal. */
export async function noticedOf(
  sim: PhonesSim,
  recap: Recap,
  today: string,
  getItem: (id: string) => Promise<ItemView>,
): Promise<Noticed[]> {
  const { start: from, end } = recap.period;
  // A period still going leaves out today, which is not over.
  const to = today > end ? end : addDays(today, -1);
  if (to < from) return [];
  const sleep = new Map<string, Counted>();
  for (const row of live(sim, "sleep", from, to)) {
    const have = sleep.get(row.day);
    const value = Math.round(asleepMinutes(row));
    if (!have || value > have.value) {
      sleep.set(row.day, { value, receipt: readingReceipt(row, "Sleep") });
    }
  }
  const steps = new Map<string, Counted>();
  for (const row of live(sim, "steps", from, to)) {
    if (row.value.type !== "steps") continue;
    const have = steps.get(row.day);
    if (!have || row.value.total > have.value) {
      steps.set(row.day, { value: row.value.total, receipt: readingReceipt(row, "Steps") });
    }
  }
  const workouts = new Map<string, Receipt[]>();
  for (const row of live(sim, "workout", from, to)) {
    const label = row.value.type === "workout" ? row.value.kind : "Workout";
    workouts.set(row.day, [...(workouts.get(row.day) ?? []), readingReceipt(row, label)]);
  }
  const body: Body = { sleep, steps, workouts };
  if (sleep.size + steps.size + workouts.size === 0) return [];

  const inRange = (day: string) => day >= from && day <= to;
  const said = async (ids: string[]): Promise<Map<string, Receipt[]>> => {
    const days = new Map<string, Receipt[]>();
    for (const id of ids) {
      const item = await getItem(id);
      const day = journalDayOf(item.keptAt);
      if (inRange(day)) days.set(day, [...(days.get(day) ?? []), receiptOf(item)]);
    }
    return days;
  };
  const topics: Topic[] = [];
  for (const entry of recap.people) {
    const days = await said(entry.with.itemIds);
    topics.push({ kind: { type: "name", with: entry.with }, days });
  }
  const parts: FeelingPart[] = ["woundUp", "litUp", "low", "settled"];
  for (const part of parts) {
    const ids = recap.checkins.filter((c) => c.feeling.part === part).map((c) => c.id);
    const days = new Map<string, Receipt[]>();
    for (const id of ids) {
      const item = await getItem(id);
      const c = recap.checkins.find((x) => x.id === id);
      if (c && inRange(c.day)) days.set(c.day, [...(days.get(c.day) ?? []), receiptOf(item)]);
    }
    topics.push({ kind: { type: "felt", part }, days });
  }
  const good = new Map<string, Receipt[]>();
  for (const g of recap.good) {
    if (!inRange(g.day)) continue;
    good.set(g.day, [...(good.get(g.day) ?? []), receiptOf(await getItem(g.id))]);
  }
  topics.push({ kind: { type: "good" }, days: good });
  return noticeRules(topics, body);
}
