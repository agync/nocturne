import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { receiptPath } from "../../components/ReceiptChip/receiptPath";
import type { Noticed, Period, Receipt, ReflectionView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";
import { noticedLine } from "./Noticed";
import { chipText } from "./Receipts";

const NOW = "2026-11-19T21:20:00Z";
const WEEK = "#/journal/look-back/week/2026-11-09";

const plain = () => createMockEngine({ now: NOW, journal: { mode: "day", delayMs: 0 } });
const phoneEngine = () => withPhones(plain(), NOW);

afterEach(() => {
  window.location.hash = "";
});

const reading = (id: string, day: string, label: string): Receipt => ({
  itemId: id,
  kind: "reading",
  source: "phone",
  label,
  // Noon of the day in the computer's zone, here Auckland, whatever zone the test runs in.
  keptAt: `${day}T12:00:00+13:00`,
  durationMs: null,
  span: null,
  unconfirmed: false,
  fromDevice: "Pixel",
});

const entry = (id: string, day: string): Receipt => ({
  ...reading(id, day, "Wednesday entry"),
  keptAt: `${day}T09:00:00.000Z`,
  kind: "text",
  source: "journal",
  fromDevice: null,
});

const SLEEP_LESS: Noticed = {
  measure: "sleep",
  topic: {
    type: "name",
    with: {
      entityId: "e1",
      name: "the Hendersons",
      kind: "person",
      role: null,
      new: false,
      itemIds: [],
    },
  },
  days: 3,
  held: 3,
  otherDays: 4,
  difference: 70,
  more: false,
  body: [
    reading("s1", "2026-11-10", "Sleep"),
    reading("s2", "2026-11-11", "Sleep"),
    reading("s3", "2026-11-12", "Sleep"),
  ],
  journal: [entry("e-a", "2026-11-10"), entry("e-b", "2026-11-11"), entry("e-c", "2026-11-12")],
};

const WALKED: Noticed = {
  measure: "workout",
  topic: { type: "felt", part: "settled" },
  days: 4,
  held: 3,
  otherDays: 3,
  difference: 0,
  more: true,
  body: [reading("w1", "2026-11-09", "Walk"), reading("w2", "2026-11-10", "Walk")],
  journal: [entry("c-a", "2026-11-09")],
};

describe("what noticing says", () => {
  const cases: Array<[Partial<Noticed>, string]> = [
    [
      {},
      "On the 3 nights before days you wrote about the Hendersons, you slept 1 h 10 min less than on other nights.",
    ],
    [
      { more: true, difference: 25 },
      "On the 3 nights before days you wrote about the Hendersons, you slept 25 min more than on other nights.",
    ],
    [
      { measure: "steps", difference: 3040, more: true },
      "You took about 3,000 more steps on the 3 days with the Hendersons in your journal.",
    ],
    [
      { measure: "steps", difference: 2000, more: false },
      "You took about 2,000 fewer steps on the 3 days with the Hendersons in your journal.",
    ],
    [
      { topic: { type: "felt", part: "low" } },
      "On the 3 nights before days you checked in as low, you slept 1 h 10 min less than on other nights.",
    ],
    [
      { topic: { type: "good" }, measure: "steps", difference: 2600, more: true },
      "You took about 2,600 more steps on the 3 days you wrote down a good thing.",
    ],
    [WALKED, "On 3 of the 4 days you did a workout, you checked in as settled."],
  ];
  it.each(cases)("%#: says what went together, never why", (change, text) => {
    expect(noticedLine({ ...SLEEP_LESS, ...change })).toBe(text);
  });
});

describe("a recap's From Health", () => {
  async function openWeek(engine = phoneEngine()) {
    setViewport(1280);
    renderApp({ hash: WEEK, engine });
    await screen.findByRole("heading", { level: 1, name: "Your week" });
    return engine;
  }

  it("follows How it felt and counts the workouts, each a receipt", async () => {
    await openWeek();
    const heading = await screen.findByRole("heading", { level: 2, name: "From Health" });
    const all = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(all.indexOf("From Health")).toBeGreaterThan(all.indexOf("How it felt"));
    const section = heading.closest("section") as HTMLElement;
    expect(
      within(section).getByRole("img", { name: /nights\. Asleep .* on an average night/ }),
    ).toBeInTheDocument();
    expect(within(section).getByText("2 walks.")).toBeInTheDocument();
    const chips = within(section).getAllByRole("link", { name: /Walk$/ });
    expect(chips.map((c) => c.textContent)).toEqual(["Tue 10 · Walk", "Fri 13 · Walk"]);
  });

  it("leaves blood pressure, heart rate and weight out", async () => {
    await openWeek();
    const section = (await screen.findByRole("heading", { name: "From Health" })).closest(
      "section",
    ) as HTMLElement;
    expect(within(section).queryByText(/blood pressure|^heart rate$|weight/i)).toBeNull();
  });

  it("is not there when no phone brings health", async () => {
    setViewport(1280);
    renderApp({ hash: WEEK, engine: plain() });
    await screen.findByRole("heading", { level: 1, name: "Your week" });
    expect(screen.queryByRole("heading", { name: "From Health" })).toBeNull();
    expect(screen.queryByText(/never why/)).toBeNull();
  });

  it("puts noticing under What tended to go together, with both sides as chips", async () => {
    const engine = phoneEngine();
    const real = engine.getReflection.bind(engine);
    engine.getReflection = async (period: Period) => {
      const view: ReflectionView = await real(period);
      return view.type === "recap" && view.recap.health
        ? {
            type: "recap",
            recap: {
              ...view.recap,
              health: { ...view.recap.health, noticed: [SLEEP_LESS, WALKED] },
            },
          }
        : view;
    };
    await openWeek(engine);
    expect(await screen.findByText(noticedLine(SLEEP_LESS))).toBeInTheDocument();
    expect(screen.getByText(noticedLine(WALKED))).toBeInTheDocument();
    expect(
      screen.getByText(
        "Shown only when it held at least three times. It says what went together, never why.",
      ),
    ).toBeInTheDocument();
    // The first two of each side, then the rest behind "+1".
    const first = screen.getByText(noticedLine(SLEEP_LESS)).parentElement as HTMLElement;
    expect(
      within(first)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Tue 10 · Sleep", "Wed 11 · Sleep", "Tue 10 · entry", "Wed 11 · entry"]);
    const more = within(first).getAllByRole("button", { name: "+1, show 1 more" });
    expect(more).toHaveLength(2);
    expect(more[0]?.textContent).toBe("+1");
    fireEvent.click(more[0] as HTMLElement);
    const revealed = within(first).getByRole("link", { name: "Thu 12 · Sleep" });
    expect(revealed).toBeInTheDocument();
    expect(revealed).toHaveFocus();
    expect(within(first).getAllByRole("button", { name: "+1, show 1 more" })).toHaveLength(1);
  });

  it("opens a night's chip on that day's Health page", async () => {
    const engine = phoneEngine();
    const real = engine.getReflection.bind(engine);
    engine.getReflection = async (period: Period) => {
      const view: ReflectionView = await real(period);
      return view.type === "recap" && view.recap.health
        ? {
            type: "recap",
            recap: { ...view.recap, health: { ...view.recap.health, noticed: [SLEEP_LESS] } },
          }
        : view;
    };
    await openWeek(engine);
    const chip = await screen.findByRole("link", { name: "Tue 10 · Sleep" });
    expect(chip.getAttribute("href")).toBe("#/journal/health?day=2026-11-10");
  });

  it("names a reading's day by the computer's own date, not the viewer's zone", () => {
    expect(receiptPath(reading("s1", "2026-11-10", "Sleep"))).toBe(
      "/journal/health?day=2026-11-10",
    );
    expect(chipText(reading("s1", "2026-11-10", "Sleep"), "weekday")).toBe("Tue 10 · Sleep");
    expect(chipText(reading("s1", "2026-11-10", "Sleep"), "date")).toBe("10 Nov · Sleep");
  });

  it("shows the mock's own sentences while the switch is on, and none once it is off", async () => {
    const on = phoneEngine();
    await openWeek(on);
    expect(await screen.findByText(/never why/)).toBeInTheDocument();
    cleanup();
    const off = phoneEngine();
    await off.setNoticing(false);
    await openWeek(off);
    await screen.findByRole("heading", { name: "From Health" });
    expect(screen.queryByText(/never why/)).toBeNull();
  });

  it("says a month by its period, with dates on its chips and no busiest weekday", async () => {
    setViewport(1280);
    renderApp({ hash: "#/journal/look-back/month/2026-11-01", engine: phoneEngine() });
    const section = (await screen.findByRole("heading", { name: "From Health" })).closest(
      "section",
    ) as HTMLElement;
    expect(
      within(section).getByRole("img", { name: /^Steps this month\. [\d,]+ on an average day\.$/ }),
    ).toBeInTheDocument();
    expect(within(section).getAllByRole("link", { name: /^\d+ Nov · / }).length).toBeGreaterThan(0);
    expect(within(section).queryByText(/most on/)).toBeNull();
  });

  it("draws a year's steps by month, not by day", async () => {
    setViewport(1280);
    // The seed has no year, so the month stands in for one.
    const base = plain();
    const real = base.getReflection.bind(base);
    base.getReflection = async (period: Period) => {
      if (period.kind !== "year") return real(period);
      const month = await real({ kind: "month", start: "2026-11-01", end: "2026-11-30" });
      if (month.type !== "recap") throw new Error("seed has no month");
      return { type: "recap", recap: { ...month.recap, period, soFar: false } };
    };
    renderApp({ hash: "#/journal/look-back/year/2026-01-01", engine: withPhones(base, NOW) });
    const section = (await screen.findByRole("heading", { name: "From Health" })).closest(
      "section",
    ) as HTMLElement;
    const chart = within(section).getByRole("img", { name: /^Steps by month this year\./ });
    expect(chart.querySelectorAll("rect").length).toBeLessThanOrEqual(12);
  });
});
