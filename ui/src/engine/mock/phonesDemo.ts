import { addDays, localDay } from "../../copy/format";
import type {
  BroughtItem,
  CalendarDetail,
  CalendarLine,
  CameInRow,
  DayHealth,
  HealthSeries,
  HourRange,
  ItemView,
  NightLine,
  PhoneSources,
  PressureReading,
  ReadingKind,
  ReadingRow,
  SeriesPoint,
  TimeSpan,
  WorkoutRow,
} from "../bindings";
import type { PhonesFixtures } from "./phones";

/** Sam's phone: the Android app that brings a calendar and health. Ana's phone: the browser. */
export const SAM = { id: "pixel", name: "Sam’s phone" };
export const ANA = { id: "work", name: "Ana’s phone" };

/** The fictional day the fixtures are built around: everything is relative to it. */
export interface Demo {
  fixtures: PhonesFixtures;
  /** Today's page lines. */
  calendarToday: CalendarLine[];
  lastNight: NightLine;
  /** The journal day's health block and the rows that go on its timeline. */
  dayHealth: DayHealth;
  /** Calendar and reading items by id. */
  items: Map<string, ItemView>;
  /** Rows for today's timeline: what kind, at what instant, and the item. */
  /** What the day lists under "From your calendar" and "From Health" in Delete this day. */
  brought: { calendar: BroughtItem[]; health: BroughtItem[] };
  timeline: Array<{ kind: "calendar" | "workout" | "woke"; at: string; item: ItemView }>;
}

const NIGHT_USUAL = { bed: "23:15", wake: "07:00" };

const RESTING = [
  57, 58, 56, 59, 60, 58, 57, 56, 58, 59, 61, 60, 58, 57, 56, 57, 58, 59, 62, 63, 61, 59, 58, 57,
  58, 60, 59, 58, 57, 58,
];
const STEPS = [6200, 4100, 3900, 7800, 8412, 11900, 5200, 6800, 7300, 4400, 9100, 6900, 5800, 8412];
const ENTRY_DAYS_BACK = [1, 2, 3, 4, 6, 8, 10, 11, 13, 14, 15, 17, 20, 22, 25, 27];
const PRESSURE: Array<[number, number, number, string]> = [
  [0, 128, 82, "07:40"],
  [2, 125, 80, "07:35"],
  [4, 121, 78, "07:45"],
  [6, 123, 80, "07:30"],
  [9, 126, 81, "07:42"],
  [11, 120, 78, "07:38"],
  [14, 124, 80, "07:36"],
  [16, 122, 79, "07:41"],
  [18, 119, 77, "07:33"],
  [21, 125, 79, "07:44"],
  [23, 118, 76, "07:31"],
  [25, 121, 78, "07:39"],
  [27, 124, 81, "07:37"],
  [29, 122, 79, "07:43"],
];
const HOURS: Array<[number, number, number]> = [
  [52, 58, 55],
  [51, 56, 53],
  [50, 55, 52],
  [50, 57, 53],
  [51, 56, 53],
  [52, 60, 55],
  [54, 72, 62],
  [60, 84, 70],
  [64, 92, 74],
  [62, 88, 72],
  [88, 148, 121],
  [66, 96, 78],
  [64, 90, 74],
  [62, 86, 72],
  [60, 84, 70],
  [62, 88, 72],
  [64, 92, 74],
  [66, 98, 76],
  [64, 90, 72],
  [62, 86, 70],
  [60, 80, 68],
  [58, 74, 64],
  [56, 68, 60],
  [54, 62, 57],
];

const clock = (hhmm: string): [number, number] => {
  const [h = 0, m = 0] = hhmm.split(":").map(Number);
  return [h, m];
};

/** The demo for a given moment. Same data every time for the same day. */
export function buildDemo(now: Date): Demo {
  const today = localDay(now);
  const at = (offset: number, hhmm: string): string => {
    const [y, mo, d] = addDays(today, offset).split("-").map(Number) as [number, number, number];
    const [h, m] = clock(hhmm);
    return new Date(y, mo - 1, d, h, m).toISOString();
  };
  const phoneName = SAM.name;
  const items = new Map<string, ItemView>();
  const item = (
    id: string,
    kind: ItemView["kind"],
    title: string,
    keptAt: string,
    extra: Partial<ItemView> = {},
  ): ItemView => {
    const made: ItemView = {
      id,
      kind,
      title,
      source: "phone",
      trust: "trusted",
      keptAt,
      status: "understood",
      reason: null,
      durationMs: null,
      pages: null,
      fileName: null,
      hasOriginal: false,
      originalPath: null,
      private: false,
      memoryCount: 1,
      understoodInMs: null,
      summary: null,
      text: null,
      language: null,
      fromDevice: phoneName,
      ...extra,
    };
    items.set(id, made);
    return made;
  };

  // Calendar: what is on today, and what came in.
  const entry = (
    id: string,
    title: string,
    offset: number,
    start: string,
    end: string,
    calendar: string,
    over: Partial<CalendarDetail> & { place?: string | null } = {},
    cameInOffset = 0,
    cameInAt = "07:42",
  ): ItemView => {
    const detail: CalendarDetail = {
      calendar,
      phone: phoneName,
      deviceId: SAM.id,
      start: at(offset, start),
      end: at(offset, end),
      allDay: false,
      zone: null,
      place: null,
      people: [],
      repeats: false,
      repeatRule: null,
      updatedAt: at(cameInOffset, cameInAt),
      was: null,
      removed: false,
      ...over,
    };
    return item(id, "calendar", title, detail.start, {
      calendar: detail,
      summary: detail.place,
      memoryCount: detail.removed ? 0 : 1,
    });
  };
  const dentist = entry("cal-dentist", "Dentist, check-up", 0, "09:00", "09:40", "Family", {
    place: "Bridge Street Surgery",
    people: ["Sam"],
  });
  const swim = entry(
    "cal-leo-swim",
    "Leo, swimming",
    0,
    "16:30",
    "17:15",
    "Family",
    {
      place: "Riverside pool",
      people: ["Sam", "Leo"],
      repeats: true,
      repeatRule: "Every Friday",
      was: {
        start: at(-2, "16:30"),
        end: at(-2, "17:15"),
        title: "Leo, swimming",
        place: "Riverside pool",
      },
    },
    0,
    "07:42",
  );
  const book = entry("cal-book-group", "Book group", 0, "19:00", "21:00", "Family", {
    place: "Priya’s",
    people: ["Sam"],
  });
  const lunch = entry(
    "cal-team-lunch",
    "Team lunch",
    6,
    "12:30",
    "14:00",
    "Work",
    { removed: true },
    0,
    "07:42",
  );
  const inset = entry(
    "cal-inset-day",
    "Inset day, no school",
    17,
    "00:00",
    "23:59",
    "School term dates",
    { allDay: true },
    -1,
    "21:10",
  );
  const eyeTest = entry(
    "cal-eye-test",
    "Gran, eye test",
    4,
    "10:15",
    "11:00",
    "Family",
    { place: "Optician" },
    -2,
    "07:58",
  );

  const calendarToday: CalendarLine[] = [dentist, swim, book].map((e) => {
    const c = e.calendar as CalendarDetail;
    return {
      itemId: e.id,
      start: c.start,
      end: c.end,
      allDay: c.allDay,
      title: e.title,
      place: c.place,
      calendar: c.calendar,
      phone: phoneName,
      zone: null,
    };
  });

  // Last night: asleep 11:50 pm to 7:05 am with one break, 6 h 40 min.
  const periods: TimeSpan[] = [
    { from: at(-1, "23:50"), to: at(0, "03:00") },
    { from: at(0, "03:35"), to: at(0, "07:05") },
  ];
  const lastNight: NightLine = {
    itemId: "read-sleep-0",
    asleepMin: 400,
    from: periods[0]?.from ?? "",
    to: periods[1]?.to ?? "",
    awakeCount: 1,
    periods,
    versus: null,
    usualBed: NIGHT_USUAL.bed,
    usualWake: NIGHT_USUAL.wake,
    phone: phoneName,
  };

  // Thirty nights. Each ends on its own day.
  const nightSpecs: Array<[string, string, string | null]> = [];
  const beds = ["23:20", "22:40", "23:10", "23:40", "23:05", "23:30", "00:10", "23:50", "22:50"];
  const wakes = ["07:30", "07:15", "06:40", "06:50", "07:00", "07:05", "07:40", "08:30", "07:10"];
  for (let back = 0; back < 30; back += 1) {
    const i = back % beds.length;
    nightSpecs.push([beds[i] ?? "23:00", wakes[i] ?? "07:00", null]);
  }
  nightSpecs[0] = ["23:50", "07:05", "03:00-03:35"];
  nightSpecs[1] = ["22:50", "07:10", null];
  nightSpecs[2] = ["00:50", "05:55", null];
  nightSpecs[3] = ["00:30", "06:10", null];

  const sleepRows: ReadingRow[] = [];
  const sleepPoints: SeriesPoint[] = [];
  nightSpecs.forEach(([bed, wake, gap], back) => {
    const day = addDays(today, -back);
    const bedOffset = clock(bed)[0] < 12 ? -back : -back - 1;
    const asleep: TimeSpan[] = [];
    let minutes = 0;
    if (gap) {
      const [g0 = "", g1 = ""] = gap.split("-");
      asleep.push({ from: at(bedOffset, bed), to: at(-back, g0) });
      asleep.push({ from: at(-back, g1), to: at(-back, wake) });
    } else {
      asleep.push({ from: at(bedOffset, bed), to: at(-back, wake) });
    }
    for (const p of asleep) minutes += (Date.parse(p.to) - Date.parse(p.from)) / 60_000;
    const id = `read-sleep-${back}`;
    item(id, "reading", "Sleep", asleep[asleep.length - 1]?.to ?? at(-back, wake));
    sleepRows.push({
      itemId: id,
      day,
      start: asleep[0]?.from ?? "",
      end: asleep[asleep.length - 1]?.to ?? null,
      value: { type: "sleep", asleep, inBedOnly: false },
      sourceDevice: "Watch",
      phone: phoneName,
      removed: false,
    });
    sleepPoints.push({ day, value: Math.round(minutes), itemId: id });
  });
  const removedSleep: ReadingRow = {
    itemId: "read-sleep-removed",
    day: addDays(today, -2),
    start: at(-3, "23:30"),
    end: at(-2, "01:40"),
    value: {
      type: "sleep",
      asleep: [{ from: at(-3, "23:30"), to: at(-2, "01:40") }],
      inBedOnly: false,
    },
    sourceDevice: "Watch",
    phone: phoneName,
    removed: true,
  };
  item("read-sleep-removed", "reading", "Sleep", removedSleep.end ?? at(-2, "01:40"));
  sleepRows.splice(2, 0, removedSleep);
  sleepPoints.sort((a, b) => a.day.localeCompare(b.day));

  const journalDays = ENTRY_DAYS_BACK.map((b) => addDays(today, -b));
  const sleepSeries: HealthSeries = {
    kind: "sleep",
    from: addDays(today, -29),
    to: today,
    points: sleepPoints,
    usual: { low: 390, high: 440 },
    usualBed: NIGHT_USUAL.bed,
    usualWake: NIGHT_USUAL.wake,
    journalDays,
  };

  const mkSeries = (kind: ReadingKind, values: number[], usual: HealthSeries["usual"]) => {
    const points: SeriesPoint[] = values.map((value, i) => {
      const day = addDays(today, -(values.length - 1 - i));
      const id = `read-${kind}-${day}`;
      item(id, "reading", kind, at(-(values.length - 1 - i), "08:00"));
      return { day, value, itemId: id };
    });
    return {
      kind,
      from: points[0]?.day ?? today,
      to: today,
      points,
      usual,
      journalDays,
    } satisfies HealthSeries;
  };
  const resting = mkSeries("restingHeartRate", RESTING, { low: 55, high: 61 });
  const steps = mkSeries("steps", STEPS, null);
  const pressureSeries: HealthSeries = {
    kind: "bloodPressure",
    from: addDays(today, -29),
    to: today,
    points: PRESSURE.map(([back, top]) => {
      const day = addDays(today, -back);
      return { day, value: top, itemId: `read-bp-${day}` };
    }),
    usual: { low: 118, high: 126 },
    pressureUsual: { top: { low: 118, high: 126 }, bottom: { low: 76, high: 82 } },
    journalDays,
  };

  const restingRows: ReadingRow[] = resting.points.map((p) => ({
    itemId: p.itemId,
    day: p.day,
    start: `${p.day}T08:00:00.000Z`,
    end: null,
    value: { type: "restingHeartRate", bpm: p.value },
    sourceDevice: "Watch",
    phone: phoneName,
    removed: false,
  }));
  const stepRows: ReadingRow[] = steps.points.map((p) => ({
    itemId: p.itemId,
    day: p.day,
    start: `${p.day}T00:00:00.000Z`,
    end: `${p.day}T23:59:00.000Z`,
    value: { type: "steps", total: p.value },
    sourceDevice: "Phone",
    phone: phoneName,
    removed: false,
  }));
  const bpRows: ReadingRow[] = PRESSURE.map(([back, top, bottom, time]) => {
    const day = addDays(today, -back);
    const id = `read-bp-${day}`;
    item(id, "reading", "Blood pressure", at(-back, time));
    return {
      itemId: id,
      day,
      start: at(-back, time),
      end: null,
      value: { type: "bloodPressure", top, bottom },
      sourceDevice: "Blood pressure monitor",
      phone: phoneName,
      removed: false,
    };
  });
  const heartRows: ReadingRow[] = Array.from({ length: 14 }, (_, back) => {
    const day = addDays(today, -back);
    const id = `read-hr-${day}`;
    item(id, "reading", "Heart rate", at(-back, "23:00"));
    const hours: HourRange[] = HOURS.map(([low, high, middle], h) => {
      const bump = back === 0 ? 0 : ((back * 7 + h * 3) % 9) - 4;
      return {
        start: at(-back, `${String(h).padStart(2, "0")}:00`),
        low: low + Math.min(bump, 0),
        high: high + Math.max(bump, 0) - (h === 10 && back % 3 !== 0 ? 40 : 0),
        middle: middle + Math.round(bump / 2),
      };
    });
    return {
      itemId: id,
      day,
      start: at(-back, "00:00"),
      end: at(-back, "23:59"),
      value: { type: "heartRate", hours, count: 1400 - back * 11 },
      sourceDevice: "Watch",
      phone: phoneName,
      removed: false,
    };
  });
  const walk = item("read-workout-walk", "reading", "Walk", at(0, "10:20"));
  const workoutRows: ReadingRow[] = [
    {
      itemId: walk.id,
      day: today,
      start: at(0, "10:20"),
      end: at(0, "11:02"),
      value: { type: "workout", kind: "Walk", lengthS: 2520, distanceM: 3100 },
      sourceDevice: "Watch",
      phone: phoneName,
      removed: false,
    },
    ...[
      ["read-workout-swim", -1, "18:00", 2100, null, "Swim"],
      ["read-workout-walk2", -3, "17:40", 1800, 2400, "Walk"],
      ["read-workout-walk3", -6, "08:10", 3000, 3900, "Walk"],
      ["read-workout-walk4", -9, "07:50", 2400, 3000, "Walk"],
    ].map(([id, back, time, length, distance, kind]) => {
      item(id as string, "reading", kind as string, at(back as number, time as string));
      return {
        itemId: id as string,
        day: addDays(today, back as number),
        start: at(back as number, time as string),
        end: null,
        value: {
          type: "workout" as const,
          kind: kind as string,
          lengthS: length as number,
          distanceM: distance as number | null,
        },
        sourceDevice: "Watch",
        phone: phoneName,
        removed: false,
      } satisfies ReadingRow;
    }),
  ];

  const pressureToday: PressureReading[] = [
    { itemId: bpRows[0]?.itemId ?? "", at: at(0, "07:40"), top: 128, bottom: 82 },
  ];
  const workoutsToday: WorkoutRow[] = [
    { itemId: walk.id, start: at(0, "10:20"), kind: "Walk", lengthS: 2520, distanceM: 3100 },
  ];
  const dayHealth: DayHealth = {
    night: lastNight,
    resting,
    steps,
    heart: {
      itemId: heartRows[0]?.itemId ?? "",
      low: 52,
      high: 148,
      middle: 74,
    },
    pressure: pressureToday,
    workouts: workoutsToday,
  };

  const cameIn = (e: ItemView, tag: CameInRow["tag"], offset: number, time: string): CameInRow => {
    const c = e.calendar as CalendarDetail;
    return {
      itemId: e.id,
      cameInAt: at(offset, time),
      title: e.title,
      day: localDay(new Date(c.start)),
      tag,
      start: c.start,
      allDay: c.allDay,
      calendar: c.calendar,
    };
  };
  const calendarCameIn: CameInRow[] = [
    cameIn(swim, "changed", 0, "07:42"),
    cameIn(book, "new", 0, "07:42"),
    cameIn(lunch, "removed", 0, "07:42"),
    cameIn(dentist, "new", -1, "21:10"),
    cameIn(inset, "new", -1, "21:10"),
    cameIn(eyeTest, "new", -2, "07:58"),
  ];

  const phones: PhoneSources[] = [
    {
      deviceId: SAM.id,
      name: SAM.name,
      inBrowser: false,
      app: { platform: "android", version: "1.2" },
      calendar: {
        taking: true,
        lastInAt: at(0, "07:42"),
        calendars: 3,
        upcoming: 23,
        kinds: [],
      },
      health: {
        taking: true,
        lastInAt: at(0, "07:42"),
        calendars: 0,
        upcoming: 0,
        kinds: ["sleep", "heartRate", "restingHeartRate", "bloodPressure", "steps", "workout"],
      },
    },
    { deviceId: ANA.id, name: ANA.name, inBrowser: true, calendar: null, health: null },
  ];

  const timeline: Demo["timeline"] = [
    { kind: "woke", at: lastNight.to, item: items.get("read-sleep-0") as ItemView },
    { kind: "calendar", at: dentist.keptAt, item: dentist },
    { kind: "workout", at: at(0, "10:20"), item: walk },
    { kind: "calendar", at: swim.keptAt, item: swim },
  ];

  const phone = phoneName;
  const brought = {
    calendar: [dentist, swim, book].map((e) => ({ itemId: e.id, title: e.title, phone })),
    health: [
      { itemId: lastNight.itemId, title: "Woke · 6 h 40 min asleep", phone },
      { itemId: walk.id, title: "Walk · 42 min", phone },
      {
        itemId: resting.points[resting.points.length - 1]?.itemId ?? "",
        title: "Resting heart rate · 58",
        phone,
      },
      {
        itemId: steps.points[steps.points.length - 1]?.itemId ?? "",
        title: "Steps · 8,412",
        phone,
      },
      { itemId: heartRows[0]?.itemId ?? "", title: "Heart rate · 52 to 148", phone },
      { itemId: bpRows[0]?.itemId ?? "", title: "Blood pressure · 128 over 82", phone },
    ],
  };

  return {
    brought,
    fixtures: {
      phones,
      cameIn: { [`${SAM.id}/calendar`]: calendarCameIn },
      series: {
        sleep: sleepSeries,
        restingHeartRate: resting,
        steps,
        bloodPressure: pressureSeries,
      },
      readings: {
        sleep: sleepRows,
        heartRate: heartRows,
        restingHeartRate: restingRows,
        bloodPressure: bpRows,
        steps: stepRows,
        workout: workoutRows,
      },
    },
    calendarToday,
    lastNight,
    dayHealth,
    items,
    timeline,
  };
}
