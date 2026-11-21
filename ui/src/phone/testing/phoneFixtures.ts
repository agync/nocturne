// What a phone brings, as the mockups show it: last night, a walk and today's appointments. The
// unit tests and the app's scripted build read the same fixtures.

import type {
  CalendarLine,
  DayHealth,
  ItemView,
  JournalDay,
  NightLine,
  PageView,
  TimelineRow,
} from "../../engine/bindings";
import { itemView, TODAY } from "./journalFixtures";

const at = (time: string, day = TODAY) => `${day}T${time}:00Z`;

export const NIGHT: NightLine = {
  itemId: "night-1",
  asleepMin: 400,
  from: at("23:50", "2026-11-18"),
  to: at("07:05"),
  awakeCount: 1,
  periods: [
    { from: at("23:50", "2026-11-18"), to: at("03:10") },
    { from: at("03:30"), to: at("07:05") },
  ],
  versus: null,
  usualBed: "23:20",
  usualWake: "07:00",
  phone: "Pixel",
};

export const CALENDAR: CalendarLine[] = [
  ["cal-1", "09:00", "Dentist, check-up", "Bridge Street Surgery"],
  ["cal-2", "16:30", "Leo, swimming", "Riverside pool"],
  ["cal-3", "19:00", "Book group", "Priya’s"],
].map(([itemId, time, title, place]) => ({
  itemId: itemId as string,
  start: at(time as string),
  end: at(time as string),
  allDay: false,
  title: title as string,
  place: place as string,
  calendar: "Family",
  phone: "Pixel",
  zone: null,
}));

const series = (kind: "restingHeartRate" | "steps", values: number[]) => ({
  kind,
  from: "2026-11-06",
  to: TODAY,
  points: values.map((value, i) => ({
    day: `2026-11-${String(6 + i).padStart(2, "0")}`,
    value,
    itemId: `${kind}-${i}`,
  })),
  usual: kind === "restingHeartRate" ? { low: 55, high: 61 } : null,
  journalDays: [TODAY],
});

export const HEALTH: DayHealth = {
  night: NIGHT,
  resting: series("restingHeartRate", [57, 58, 56, 59, 60, 58, 57, 56, 58, 59, 61, 57, 58, 58]),
  steps: series(
    "steps",
    [6200, 7100, 5400, 8800, 6900, 7300, 4100, 9100, 6600, 7000, 5800, 6400, 7900, 8412],
  ),
  heart: { itemId: "hr-1", low: 52, high: 148, middle: 74 },
  pressure: [],
  workouts: [
    { itemId: "walk-1", start: at("10:20"), kind: "Walk", lengthS: 2520, distanceM: 3100 },
  ],
};

function phoneRow(kind: TimelineRow["kind"], item: ItemView): TimelineRow {
  return {
    kind,
    part: "morning",
    item,
    checkin: null,
    good: null,
    feeling: null,
    mentions: [],
    leftOut: false,
    moveTo: null,
  };
}

/** Today's journal with what the phone brought: the night, a walk and two appointments. */
export function withPhones(day: JournalDay): JournalDay {
  const rows: TimelineRow[] = [
    phoneRow("woke", itemView({ id: "night-1", kind: "reading", keptAt: at("07:05") })),
    phoneRow(
      "calendar",
      itemView({ id: "cal-1", kind: "calendar", keptAt: at("09:00"), title: "Dentist, check-up" }),
    ),
    phoneRow("workout", itemView({ id: "walk-1", kind: "reading", keptAt: at("10:20") })),
    phoneRow(
      "calendar",
      itemView({ id: "cal-2", kind: "calendar", keptAt: at("16:30"), title: "Leo, swimming" }),
    ),
  ];
  const all = [...day.rows, ...rows].sort((a, b) => a.item.keptAt.localeCompare(b.item.keptAt));
  return { ...day, rows: all, health: HEALTH };
}

export function withCalendar(page: PageView): PageView {
  return { ...page, calendarToday: CALENDAR, moreCalendarToday: 0, lastNight: NIGHT };
}
