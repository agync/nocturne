import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { formatDay } from "../../copy/format";
import { EngineError } from "../../engine/client";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import type { FakePlatform } from "../../test/render";
import { renderApp, setViewport } from "../../test/render";

const NOW = "2026-11-13T07:02:00Z";

const phoneEngine = () => withPhones(createMockEngine({ now: NOW }), NOW);

async function open(slug: string, engine = phoneEngine()) {
  setViewport(1280);
  const view = renderApp({ hash: `#/journal/health/${slug}`, engine });
  await screen.findByRole("heading", { level: 1 });
  return { ...view, engine };
}

afterEach(() => {
  window.location.hash = "";
});

describe("the health kind pages", () => {
  it("opens on sleep with a tab for each kind the phone brings, and none for weight", async () => {
    setViewport(1280);
    renderApp({ hash: "#/journal/health", engine: phoneEngine() });
    expect(await screen.findByRole("heading", { level: 1, name: "Sleep" })).toBeInTheDocument();
    const tabs = within(screen.getByRole("navigation", { name: "Health" }));
    expect(tabs.getAllByRole("link").map((a) => a.textContent)).toEqual([
      "Sleep",
      "Heart rate",
      "Resting heart rate",
      "Blood pressure",
      "Steps",
      "Workouts",
    ]);
    expect(tabs.getByRole("link", { name: "Sleep" })).toHaveAttribute("aria-current", "page");
    expect(tabs.getByRole("link", { name: "Steps" })).not.toHaveAttribute("aria-current");
    expect(
      screen
        .getAllByRole("link", { name: "Journal" })
        .some((a) => a.getAttribute("href") === "#/journal"),
    ).toBe(true);
  });

  it("shows NotFound for an unknown kind", async () => {
    setViewport(1280);
    renderApp({ hash: "#/journal/health/mood", engine: phoneEngine() });
    expect(
      await screen.findByRole("heading", { level: 1, name: "That page isn’t here" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sleep" })).toBeNull();
  });

  it("draws 30 nights as a strip named by its sentence, and lists the same nights", async () => {
    await open("sleep");
    const chart = await screen.findByRole("img", {
      name: /^30 nights\. Asleep .* on an average night/,
    });
    expect(chart).toHaveAttribute("data-chart", "nights");
    expect(
      screen.getByText(
        "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show as a list" }));
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("row")).toHaveLength(31);
    expect(within(table).getByText("6 h 40 min")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show as a chart" }));
    expect(screen.getByRole("img", { name: /^30 nights/ })).toBeInTheDocument();
  });

  it("keeps a reading removed in Health out of the chart and in the secondary ink", async () => {
    await open("sleep");
    await screen.findByRole("img", { name: /^30 nights/ });
    const removed = screen.getByText("Removed in Health · not in charts").closest("li");
    expect(removed).toHaveAttribute("data-removed", "true");
    expect(within(removed as HTMLElement).queryByRole("link")).toBeNull();
    const rows = screen.getAllByRole("listitem").filter((li) => li.className.includes("row"));
    expect(rows).toHaveLength(31);
    const kept = rows.find((li) => li.getAttribute("data-removed") === "false") as HTMLElement;
    expect(within(kept).getByRole("link", { name: "Journal ›" }).getAttribute("href")).toMatch(
      /^#\/journal\/\d{4}-\d{2}-\d{2}$/,
    );
    expect(kept).toHaveTextContent("Watch, through Health on Sam’s phone");
  });

  it("draws sleep for 90 days as a line on the usual band", async () => {
    await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Last 90 days" }));
    expect(screen.getByRole("button", { name: "Last 90 days" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    const chart = await screen.findByRole("img", {
      name: /^Sleep, .* today\. Your usual is .* to /,
    });
    expect(chart).toHaveAttribute("data-chart", "line");
  });

  it("draws heart rate by hour with the day's range and the workout that explains the peak", async () => {
    await open("heart-rate");
    expect(
      await screen.findByRole("img", {
        name: /^Heart rate today, \d+ to \d+ beats a minute, mostly around \d+\.$/,
      }),
    ).toHaveAttribute("data-chart", "range");
    expect(
      await screen.findByRole("img", {
        name: /^Heart rate by hour today\. Highest between .*, during a walk\.$/,
      }),
    ).toHaveAttribute("data-chart", "hours");
    await userEvent.click(screen.getByRole("button", { name: "Show as a list" }));
    expect(within(screen.getByRole("table")).getAllByRole("row").length).toBeGreaterThan(10);
  });

  it("draws resting heart rate as a line with the usual", async () => {
    await open("resting-heart-rate");
    expect(
      await screen.findByRole("img", {
        name: /^Resting heart rate, \d+ beats a minute today\. Your usual is \d+ to \d+\.$/,
      }),
    ).toHaveAttribute("data-chart", "line");
    expect(screen.queryByText("Your usual shows after two weeks of readings.")).toBeNull();
  });

  it("shows the waiting line and no usual before two weeks of readings", async () => {
    const engine = phoneEngine();
    const series = engine.sim.fixtures.series.restingHeartRate;
    if (!series) throw new Error("fixture");
    series.points = series.points.slice(-5);
    series.usual = null;
    await open("resting-heart-rate", engine);
    const chart = await screen.findByRole("img", {
      name: /^Resting heart rate, \d+ beats a minute today\.$/,
    });
    expect(chart.getAttribute("aria-label")).not.toContain("usual");
    expect(screen.getByText("Your usual shows after two weeks of readings.")).toBeInTheDocument();
  });

  it("draws blood pressure as a strip with two usual bands once there are two weeks", async () => {
    await open("blood-pressure");
    const chart = await screen.findByRole("img", { name: /^128 over 82 this morning\./ });
    expect(chart).toHaveAttribute("data-chart", "pressure");
    await userEvent.click(screen.getByRole("button", { name: "Show as a list" }));
    expect(within(screen.getByRole("table")).getAllByText("128 over 82")).not.toHaveLength(0);
  });

  it("draws steps as day bars with today's total", async () => {
    await open("steps");
    const chart = await screen.findByRole("img", {
      name: /^[\d,]+ steps today\. [\d,]+ on an average day in the last 30 days\.$/,
    });
    expect(chart).toHaveAttribute("data-chart", "bars");
  });

  it("counts workouts by kind and lists them with their length and distance", async () => {
    await open("workouts");
    expect(await screen.findByText("4 walks, 1 swim.")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText("Walk · 42 min · 3.1 km")).toBeInTheDocument();
    expect(screen.getByText("Swim · 35 min")).toBeInTheDocument();
  });

  it("draws weight on a scale that does not start at zero, with its own unit", async () => {
    const engine = phoneEngine();
    const days = ["2026-11-11", "2026-11-12", "2026-11-13"];
    const phone = engine.sim.fixtures.phones[0];
    phone?.health?.kinds.push("weight");
    engine.sim.fixtures.series.weight = {
      kind: "weight",
      from: "2026-10-15",
      to: "2026-11-13",
      points: days.map((day, i) => ({ day, value: 78 + i, itemId: `w${i}` })),
      usual: null,
      journalDays: [],
    };
    engine.sim.fixtures.readings.weight = days.map((day, i) => ({
      itemId: `w${i}`,
      day,
      start: `${day}T07:30:00Z`,
      end: null,
      value: { type: "weight", value: 78 + i, unit: "kg" },
      sourceDevice: "Scale",
      phone: "Sam’s phone",
      removed: false,
    }));
    await open("weight", engine);
    expect(await screen.findByRole("img", { name: /^Weight, 80 kg today\./ })).toBeInTheDocument();
    expect(screen.getAllByText("80 kg").length).toBeGreaterThan(0);
    expect(screen.getByRole("link", { name: "Weight" })).toHaveAttribute("aria-current", "page");
  });

  it("prints through the platform", async () => {
    const { platform } = await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Print these readings" }));
    await waitFor(() => expect((platform as FakePlatform).printed()).toBe(1));
  });
});

describe("Delete readings", () => {
  it("previews what a delete takes out, and deletes it", async () => {
    const { engine } = await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete readings?" });
    expect(within(dialog).getByRole("button", { name: "Sleep" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(dialog).getByRole("button", { name: "Steps" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(within(dialog).getByRole("button", { name: "All of them" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(
      await within(dialog).findByText("31 readings of sleep from Sam’s phone"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "3 lines in your reflections and recaps cite them and will be taken out.",
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText("1 sentence about what went together will be worked out again."),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/Health on your phone isn’t touched\./)).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "Steps" }));
    expect(
      await within(dialog).findByText("45 readings of sleep and steps from Sam’s phone"),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Steps" }));
    await within(dialog).findByText("31 readings of sleep from Sam’s phone");

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await screen.findByText("No readings in this range.");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(engine.sim.fixtures.readings.sleep).toEqual([]);
    expect(engine.sim.fixtures.readings.steps?.length).toBeGreaterThan(0);
  });

  it("keeps everything when asked to keep them", async () => {
    const { engine } = await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep them" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(engine.sim.fixtures.readings.sleep?.length).toBe(31);
  });

  it("previews only what is before a chosen day, and waits for the day", async () => {
    const { engine } = await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Before a date…" }));
    expect(within(dialog).getByRole("button", { name: "Delete" })).toBeDisabled();
    const date = within(dialog).getByLabelText("Before this day");
    await userEvent.type(date, "2026-11-01");
    expect(
      await within(dialog).findByText(/^\d+ readings of sleep from Sam’s phone$/),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const left = engine.sim.fixtures.readings.sleep ?? [];
    expect(left.length).toBeGreaterThan(0);
    expect(left.every((r) => r.day >= "2026-11-01")).toBe(true);
  });
});

describe("Delete for one reading", () => {
  it("opens from the row's menu and deletes that reading alone", async () => {
    const { engine } = await open("steps");
    const before = (await engine.listReadings("steps", "2026-01-01", "2026-12-31")).length;
    const first = (await screen.findAllByRole("button", { name: /^More about the reading on/ }))[0];
    await userEvent.click(first as HTMLElement);
    await userEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this reading?" });
    expect(within(dialog).queryByRole("button", { name: "All of them" })).toBeNull();
    expect(
      await within(dialog).findByText("1 reading of steps from Sam’s phone"),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(async () =>
      expect((await engine.listReadings("steps", "2026-01-01", "2026-12-31")).length).toBe(
        before - 1,
      ),
    );
  });

  it("says what stays when something the person kept says the same", async () => {
    const { engine } = await open("sleep");
    const real = engine.previewPhoneDelete.bind(engine);
    engine.previewPhoneDelete = async (scope) => ({ ...(await real(scope)), sharedMemories: 2 });
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete readings?" });
    expect(
      await within(dialog).findByText(
        "2 things Nocturne learned also come from things you kept. They stay; only the phone’s receipt goes.",
      ),
    ).toBeInTheDocument();
  });

  it("uses the engine's usual for blood pressure and for the night", async () => {
    await open("blood-pressure");
    expect(
      await screen.findByRole("img", {
        name: "128 over 82 this morning. Your usual is 118 to 126 over 76 to 82.",
      }),
    ).toBeInTheDocument();
  });
});

const failing = async () => {
  throw new Error("down");
};

describe("when the readings do not load", () => {
  it("says so with Try again when the phones do not load, and recovers", async () => {
    const engine = phoneEngine();
    const real = engine.getPhoneSources.bind(engine);
    engine.getPhoneSources = failing;
    setViewport(1280);
    renderApp({ hash: "#/journal/health/sleep", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t open your readings. What you kept is safe.",
    );
    engine.getPhoneSources = real;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Sleep" })).toBeInTheDocument();
  });

  it("shows an error, not an empty page, when the readings fail", async () => {
    const engine = phoneEngine();
    const real = engine.listReadings.bind(engine);
    engine.listReadings = failing;
    await open("steps", engine);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t open your readings.");
    expect(screen.queryByText("No readings in this range.")).toBeNull();
    engine.listReadings = real;
    await userEvent.click(within(alert).getByRole("button", { name: "Try again" }));
    await screen.findByRole("img", { name: /steps today/ });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows an error when the series fails", async () => {
    const engine = phoneEngine();
    engine.getHealthSeries = failing;
    await open("steps", engine);
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn’t open your readings");
    expect(screen.queryByText("No readings in this range.")).toBeNull();
  });
});

describe("Delete readings, when things go wrong", () => {
  it("says when the list could not be got ready, and keeps Delete off", async () => {
    const engine = phoneEngine();
    engine.previewPhoneDelete = failing;
    await open("sleep", engine);
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t get the list ready. Nothing has been deleted.",
    );
    expect(within(dialog).getByRole("button", { name: "Delete" })).toBeDisabled();
  });

  it("says when the delete did not go through, and can be tried again", async () => {
    const { engine } = await open("sleep");
    const real = engine.deletePhoneData.bind(engine);
    engine.deletePhoneData = failing;
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("31 readings of sleep from Sam’s phone");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "That didn’t go through. Nothing was deleted.",
    );
    expect(engine.sim.fixtures.readings.sleep?.length).toBe(31);
    engine.deletePhoneData = real;
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(engine.sim.fixtures.readings.sleep).toEqual([]);
  });

  it("previews again and says so when the answer has expired", async () => {
    const { engine } = await open("sleep");
    const real = engine.deletePhoneData.bind(engine);
    let first = true;
    engine.deletePhoneData = async (token, stop) => {
      if (first) {
        first = false;
        throw new EngineError("tokenExpired", "preview again");
      }
      return real(token, stop);
    };
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("31 readings of sleep from Sam’s phone");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(
      await within(dialog).findByText(
        "That list was out of date, so Nocturne refreshed it. Check it, then delete.",
      ),
    ).toBeInTheDocument();
    expect(engine.sim.fixtures.readings.sleep?.length).toBe(31);
    await within(dialog).findByText("31 readings of sleep from Sam’s phone");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(engine.sim.fixtures.readings.sleep).toEqual([]);
  });

  it("ignores a preview answer that comes after a newer choice", async () => {
    const { engine } = await open("sleep");
    const real = engine.previewPhoneDelete.bind(engine);
    const held: Array<() => void> = [];
    engine.previewPhoneDelete = (scope) =>
      new Promise((resolve) => {
        held.push(() => void real(scope).then(resolve));
      });
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Steps" }));
    await waitFor(() => expect(held.length).toBe(2));
    held[1]?.();
    await within(dialog).findByText("45 readings of sleep and steps from Sam’s phone");
    held[0]?.();
    await new Promise((r) => setTimeout(r, 20));
    expect(
      within(dialog).getByText("45 readings of sleep and steps from Sam’s phone"),
    ).toBeInTheDocument();
  });

  it("brings focus back to the button that opened it, kept or deleted", async () => {
    await open("steps");
    const foot = await screen.findByRole("button", { name: "Delete readings…" });
    await userEvent.click(foot);
    await userEvent.click(await screen.findByRole("button", { name: "Keep them" }));
    await waitFor(() => expect(foot).toHaveFocus());

    const more = (await screen.findAllByRole("button", { name: /^More about the reading on/ }))[1];
    await userEvent.click(more as HTMLElement);
    await userEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Keep them" }));
    await waitFor(() => expect(more).toHaveFocus());

    await userEvent.click(more as HTMLElement);
    await userEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("1 reading of steps from Sam’s phone");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Delete readings…" })).toHaveFocus(),
    );
  });
});

describe("receipts and the selected day", () => {
  it("makes each value in the list a link to that day's readings", async () => {
    await open("steps");
    await userEvent.click(await screen.findByRole("button", { name: "Show as a list" }));
    const table = screen.getByRole("table");
    expect(within(table).getByRole("columnheader", { name: "Steps" })).toBeInTheDocument();
    const cells = within(table).getAllByRole("link");
    expect(cells.length).toBeGreaterThan(10);
    expect(cells[0]?.getAttribute("href")).toBe("#/journal/health/steps?day=2026-11-13");
  });

  it("draws a link for each day on the charts", async () => {
    await open("steps");
    const chart = await screen.findByRole("img", { name: /steps today/ });
    const links = [...(chart.parentElement?.querySelectorAll("a") ?? [])];
    expect(links.length).toBeGreaterThan(10);
    expect(links[0]?.getAttribute("tabindex")).toBe("-1");
    expect(links.map((a) => a.getAttribute("href"))).toContain(
      "#/journal/health/steps?day=2026-11-13",
    );
  });

  it("marks the selected day in the list, the table and the chart", async () => {
    await open("sleep?day=2026-11-10");
    await screen.findByRole("img", { name: /^30 nights/ });
    const current = document.querySelectorAll('li[aria-current="true"]');
    expect(current).toHaveLength(1);
    expect(current[0]).toHaveTextContent(formatDay("2026-11-10"));
    const inked = document.querySelectorAll('rect[data-tone="ink"]');
    expect(inked.length).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole("button", { name: "Show as a list" }));
    const row = within(screen.getByRole("table"))
      .getAllByRole("row")
      .find((r) => r.getAttribute("aria-current") === "true");
    expect(row).toBeDefined();
  });

  it("ignores a day that is not a day, and keeps the day and phone moving between tabs", async () => {
    await open("steps?day=nonsense");
    await screen.findByRole("img", { name: /steps today/ });
    expect(document.querySelector('[aria-current="true"][class*=row]')).toBeNull();
  });

  it("keeps the day and the phone when moving between kinds", async () => {
    await open("steps?day=2026-11-10&phone=pixel");
    await screen.findByRole("img", { name: /steps today/ });
    const tab = within(screen.getByRole("navigation", { name: "Health" })).getByRole("link", {
      name: "Sleep",
    });
    expect(tab.getAttribute("href")).toBe("#/journal/health/sleep?day=2026-11-10&phone=pixel");
  });
});

describe("more than one phone brings health", () => {
  const second = (engine: ReturnType<typeof phoneEngine>) => {
    const sam = engine.sim.fixtures.phones[0];
    if (!sam?.health) throw new Error("fixture");
    engine.sim.fixtures.phones.push({
      ...sam,
      deviceId: "phone-kit",
      name: "Kit’s phone",
      health: { ...sam.health, kinds: ["sleep", "weight"] },
    });
    return sam.deviceId;
  };

  it("lets the person choose the phone to delete from, and only then", async () => {
    const lone = await open("sleep");
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    expect(screen.queryByRole("group", { name: "Which phone" })).toBeNull();
    lone.unmount();

    const engine = phoneEngine();
    second(engine);
    await open("sleep", engine);
    await userEvent.click(await screen.findByRole("button", { name: "Delete readings…" }));
    const dialog = await screen.findByRole("dialog");
    const which = within(dialog).getByRole("group", { name: "Which phone" });
    expect(within(which).getByRole("button", { name: "Sam’s phone" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await userEvent.click(within(which).getByRole("button", { name: "Kit’s phone" }));
    expect(
      await within(dialog).findByText("31 readings of sleep from Kit’s phone"),
    ).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Steps" })).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Weight" })).toBeInTheDocument();
  });

  it("drives the tabs from the phone in the address", async () => {
    const engine = phoneEngine();
    second(engine);
    await open("sleep?phone=phone-kit", engine);
    const tabs = within(screen.getByRole("navigation", { name: "Health" }));
    expect(tabs.getAllByRole("link").map((a) => a.textContent)).toEqual(["Sleep", "Weight"]);
  });
});
