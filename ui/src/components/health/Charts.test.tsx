import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { NightLine } from "../../engine/bindings";
import {
  BandLine,
  DayBars,
  DayRange,
  HourBars,
  NightBar,
  NightsStrip,
  ReadingsStrip,
  ShowAsList,
} from "./index";
import {
  beatsText,
  hoursSentence,
  lineSentence,
  nightSentence,
  nightsSentence,
  pressureSentence,
  rangeSentence,
  stepsPeriod,
  stepsSentence,
  usualText,
  workoutsSentence,
} from "./sentence";

const night: NightLine = {
  itemId: "n",
  asleepMin: 400,
  from: "2026-11-12T23:50:00Z",
  to: "2026-11-13T07:05:00Z",
  awakeCount: 1,
  periods: [
    { from: "2026-11-12T23:50:00Z", to: "2026-11-13T03:00:00Z" },
    { from: "2026-11-13T03:35:00Z", to: "2026-11-13T07:05:00Z" },
  ],
  versus: null,
  usualBed: "23:15",
  usualWake: "07:00",
  phone: "Sam’s phone",
};

describe("the sentences", () => {
  it("say what the design says, in the person's own numbers", () => {
    expect(nightSentence(night)).toBe("6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once.");
    expect(nightSentence({ ...night, awakeCount: 0 })).toBe(
      "6 h 40 min asleep, from 11:50 pm to 7:05 am.",
    );
    expect(nightSentence({ ...night, awakeCount: 3 })).toContain("awake 3 times");
    expect(nightsSentence([410, 330, 485, 305, 500, 420, 420])).toBe(
      "7 nights. Asleep 6 h 50 min on an average night, between 5 h 05 min and 8 h 20 min.",
    );
    expect(
      lineSentence({
        label: "Resting heart rate",
        value: "58",
        day: "2026-11-13",
        today: "2026-11-13",
        band: { low: 55, high: 61 },
      }),
    ).toBe("Resting heart rate, 58 today. Your usual is 55 to 61.");
    expect(
      lineSentence({
        label: "Resting heart rate",
        value: "58",
        day: "2026-11-13",
        today: "2026-11-13",
        band: null,
      }),
    ).toBe("Resting heart rate, 58 today.");
    expect(
      rangeSentence({ low: 52, high: 148, middle: 74, day: "2026-11-13", today: "2026-11-13" }),
    ).toBe("Heart rate today, 52 to 148 beats a minute, mostly around 74.");
    expect(
      hoursSentence({ day: "2026-11-13", today: "2026-11-13", highest: 10, workout: "walk" }),
    ).toBe("Heart rate by hour today. Highest between 10 and 11 am, during a walk.");
    expect(
      hoursSentence({ day: "2026-11-13", today: "2026-11-13", highest: 17, workout: null }),
    ).toBe("Heart rate by hour today. Highest between 5 and 6 pm.");
    expect(
      pressureSentence({
        top: 128,
        bottom: 82,
        when: "this morning",
        bandTop: { low: 118, high: 126 },
        bandBottom: { low: 76, high: 82 },
      }),
    ).toBe("128 over 82 this morning. Your usual is 118 to 126 over 76 to 82.");
    expect(
      stepsSentence({
        steps: 8412,
        day: "2026-11-13",
        today: "2026-11-13",
        average: 6900,
        days: 14,
      }),
    ).toBe("8,412 steps today. 6,900 on an average day this fortnight.");
  });

  it("spells the unit the first time a number is read", () => {
    expect(beatsText(58)).toBe("58 beats a minute");
    expect(
      lineSentence({
        label: "Resting heart rate",
        value: beatsText(58),
        day: "2026-11-13",
        today: "2026-11-13",
        band: { low: 55, high: 61 },
      }),
    ).toBe("Resting heart rate, 58 beats a minute today. Your usual is 55 to 61.");
    expect(
      stepsSentence({ steps: 1, day: "2026-11-13", today: "2026-11-13", average: 900, days: 14 }),
    ).toBe("1 step today. 900 on an average day this fortnight.");
  });

  it("names the steps period by the days it covers", () => {
    expect(stepsPeriod(14)).toBe("this fortnight");
    expect(stepsPeriod(30)).toBe("in the last 30 days");
    expect(stepsPeriod(90)).toBe("in the last 90 days");
  });

  it("counts workouts by kind through the plural rules, and the rest as workouts", () => {
    expect(workoutsSentence(["Walk", "Walk", "Swim", "Walk", "Walk"])).toBe("4 walks, 1 swim.");
    expect(workoutsSentence(["Run", "Cycle", "Hike", "Hike"])).toBe("1 run, 1 cycle, 2 hikes.");
    expect(workoutsSentence(["Yoga", "Walk", "Rowing"])).toBe("2 workouts, 1 walk.");
    expect(workoutsSentence(["Yoga"])).toBe("1 workout.");
    expect(workoutsSentence([])).toBe("");
  });

  it("rounds the usual to whole numbers", () => {
    expect(usualText({ low: 54.6, high: 61.2 })).toBe("Your usual is 55 to 61");
    expect(
      pressureSentence({
        top: 128,
        bottom: 82,
        when: "today",
        bandTop: { low: 121, high: 124.8 },
        bandBottom: { low: 77.6, high: 80.4 },
      }),
    ).toBe("128 over 82 today. Your usual is 121 to 125 over 78 to 80.");
  });

  it("never says a verdict", () => {
    const all = [
      nightSentence(night),
      rangeSentence({ low: 52, high: 148, middle: 74, day: "d", today: "d" }),
      stepsSentence({ steps: 1, day: "d", today: "d", average: 5000, days: 14 }),
    ].join(" ");
    expect(all).not.toMatch(/poor|good|normal|healthy|risk|high blood|elevated|score/i);
  });
});

describe("the charts", () => {
  it("each carry their sentence as the accessible name", () => {
    const sentence = "A sentence.";
    const { container } = render(
      <div>
        <NightBar periods={night.periods} sentence={sentence} />
        <NightsStrip nights={[]} sentence={sentence} />
        <BandLine
          points={[
            { day: "2026-11-12", value: 57, itemId: "a" },
            { day: "2026-11-13", value: 58, itemId: "b" },
          ]}
          from="2026-11-12"
          to="2026-11-13"
          band={{ low: 55, high: 61 }}
          min={40}
          max={80}
          sentence={sentence}
        />
        <DayRange low={52} high={148} middle={74} sentence={sentence} />
        <HourBars hours={[]} sentence={sentence} />
        <ReadingsStrip readings={[]} from="2026-11-01" to="2026-11-13" sentence={sentence} />
        <DayBars points={[]} from="2026-11-01" to="2026-11-13" sentence={sentence} />
      </div>,
    );
    const charts = within(container).getAllByRole("img", { name: sentence });
    expect(charts).toHaveLength(7);
  });

  it("draw the usual as a band in the accent tint and mark nothing as good or bad", () => {
    const { container } = render(
      <BandLine
        points={[
          { day: "2026-11-12", value: 57, itemId: "a" },
          { day: "2026-11-13", value: 58, itemId: "b" },
        ]}
        from="2026-11-12"
        to="2026-11-13"
        band={{ low: 55, high: 61 }}
        min={40}
        max={80}
        sentence="s"
      />,
    );
    expect(container.querySelector("rect[class*=band]")).not.toBeNull();
    expect(container.innerHTML).not.toMatch(/stroke="|fill="|style=/);
  });

  it("draw no band before there is a usual", () => {
    const { container } = render(
      <BandLine
        points={[{ day: "2026-11-13", value: 58, itemId: "b" }]}
        from="2026-11-13"
        to="2026-11-13"
        band={null}
        min={40}
        max={80}
        sentence="s"
      />,
    );
    expect(container.querySelector("rect[class*=band]")).toBeNull();
  });
});

describe("receipts in the charts", () => {
  const points = [
    { day: "2026-11-12", value: 57, itemId: "a" },
    { day: "2026-11-13", value: 58, itemId: "b" },
  ];
  const href = (day: string) => `#/journal/health/x?day=${day}`;

  it("give each day's point a link for a pointer, out of the tab order", () => {
    const { container } = render(
      <div>
        <BandLine
          points={points}
          from="2026-11-12"
          to="2026-11-13"
          band={null}
          min={40}
          max={80}
          sentence="s"
          href={href}
        />
        <DayBars points={points} from="2026-11-12" to="2026-11-13" sentence="s" href={href} />
      </div>,
    );
    const links = [...container.querySelectorAll("a")];
    expect(links.map((a) => a.getAttribute("href"))).toEqual([
      "#/journal/health/x?day=2026-11-12",
      "#/journal/health/x?day=2026-11-13",
      "#/journal/health/x?day=2026-11-12",
      "#/journal/health/x?day=2026-11-13",
    ]);
    expect(links.every((a) => a.getAttribute("tabindex") === "-1")).toBe(true);
  });

  it("leave a chart without links when it is given no address", () => {
    const { container } = render(
      <BandLine
        points={points}
        from="2026-11-12"
        to="2026-11-13"
        band={null}
        min={40}
        max={80}
        sentence="s"
      />,
    );
    expect(container.querySelector("a")).toBeNull();
  });
});

describe("Show as a list", () => {
  it("swaps the chart for a table of the same numbers, and back", async () => {
    render(
      <ShowAsList
        sentence="2 nights."
        chart={<svg role="img" aria-label="2 nights." />}
        columns={["Day", "Asleep"]}
        rows={[
          { key: "a", cells: ["Fri 13", "6 h 40 min"] },
          { key: "b", cells: ["Thu 12", "8 h 20 min"] },
        ]}
      />,
    );
    expect(screen.getByRole("img", { name: "2 nights." })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show as a list" }));
    const table = screen.getByRole("table", { name: "2 nights." });
    expect(within(table).getAllByRole("row")).toHaveLength(3);
    expect(within(table).getByText("6 h 40 min")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show as a chart" }));
    expect(screen.getByRole("img", { name: "2 nights." })).toBeInTheDocument();
  });
});
