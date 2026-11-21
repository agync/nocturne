import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NOW, openPhones, phonesEngine } from "./testing";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(NOW));
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.location.hash = "";
});

const PATH = "/settings/phones";

const block = (name: string) =>
  within(screen.getByRole("heading", { level: 2, name }).closest("li") as HTMLElement);

describe("from your phones", () => {
  it("shows each phone with its kind and what it brings", async () => {
    await openPhones(PATH, "From your phones");
    expect(
      screen.getByText(
        "What your phones bring to this computer. Your phones read it; nothing here changes it on the phone.",
      ),
    ).toBeInTheDocument();
    const sam = block("Sam’s phone");
    expect(sam.getByText("Android app · Nocturne 1.2")).toBeInTheDocument();
    expect(sam.getByText("3 calendars · last came in 7:42 am")).toBeInTheDocument();
    expect(sam.getByText("23 appointments in the next two weeks")).toBeInTheDocument();
    expect(
      sam.getByText(/^Sleep, heart rate, resting heart rate.* · last came in 7:42 am$/),
    ).toBeInTheDocument();
    const ana = block("Ana’s phone");
    expect(ana.getByText("In the browser")).toBeInTheDocument();
    expect(
      ana.getByText(
        "A phone in the browser can’t bring its calendar or health. The Nocturne app can.",
      ),
    ).toBeInTheDocument();
  });

  it("links to what came in and to the readings", async () => {
    await openPhones(PATH, "From your phones");
    const sam = block("Sam’s phone");
    expect(sam.getByRole("link", { name: /See what came in/ })).toHaveAttribute(
      "href",
      "#/settings/phones/came-in/pixel/calendar",
    );
    expect(sam.getByRole("link", { name: /See readings/ })).toHaveAttribute(
      "href",
      "#/journal/health?phone=pixel",
    );
  });

  it("stops taking the calendar after asking, and takes it again", async () => {
    const { engine } = await openPhones(PATH, "From your phones");
    const set = vi.spyOn(engine, "setPhoneSource");
    await userEvent.click(
      screen.getByRole("button", { name: "Stop taking the calendar from Sam’s phone" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Stop taking the calendar from Sam’s phone?",
    });
    expect(
      within(dialog).getByText(
        "What already came in stays. Sam’s phone is told next time it connects, and stops sending.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep taking it" }));
    expect(set).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "Stop taking the calendar from Sam’s phone" }),
    );
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop" }),
    );
    await waitFor(() => expect(set).toHaveBeenCalledWith("pixel", "calendar", false));
    const again = await screen.findByRole("button", {
      name: "Take the calendar from Sam’s phone again",
    });
    expect(block("Sam’s phone").getByText("Stopped")).toBeInTheDocument();
    await waitFor(() => expect(again).toHaveFocus());
    await userEvent.click(again);
    await waitFor(() => expect(set).toHaveBeenCalledWith("pixel", "calendar", true));
    expect(
      await screen.findByRole("button", { name: "Stop taking the calendar from Sam’s phone" }),
    ).toBeInTheDocument();
  });

  it("switches noticing and the two page options", async () => {
    const { engine } = await openPhones(PATH, "From your phones");
    const noticing = vi.spyOn(engine, "setNoticing");
    const page = vi.spyOn(engine, "setPhonePageOptions");
    await userEvent.click(screen.getByRole("switch", { name: "Notice what goes together" }));
    expect(noticing).toHaveBeenCalledWith(false);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Notice what goes together" })).toHaveAttribute(
        "aria-checked",
        "false",
      ),
    );
    expect(screen.getByText("On your page")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Your calendar today" }));
    expect(page).toHaveBeenLastCalledWith({ calendarToday: false, sleepLastNight: true });
    await userEvent.click(screen.getByRole("switch", { name: "Sleep last night" }));
    expect(page).toHaveBeenLastCalledWith({ calendarToday: false, sleepLastNight: false });
    expect(
      screen.getByText(
        "Calendar and health come only through Away from home, in locked messages. Nothing else is downloaded or sent.",
      ),
    ).toBeInTheDocument();
  });

  it("shows Notice what goes together once, however many phones bring health", async () => {
    const engine = phonesEngine();
    const first = engine.sim.fixtures.phones[0];
    if (!first) throw new Error("fixture");
    engine.sim.fixtures.phones.push({ ...structuredClone(first), deviceId: "second", name: "Mia" });
    await openPhones(PATH, "From your phones", engine);
    await screen.findByText("Mia");
    expect(screen.getAllByRole("switch", { name: "Notice what goes together" })).toHaveLength(1);
  });

  it("leaves out Notice what goes together when no phone brings health", async () => {
    const engine = phonesEngine();
    for (const phone of engine.sim.fixtures.phones) phone.health = null;
    await openPhones(PATH, "From your phones", engine);
    expect(screen.queryByRole("switch", { name: "Notice what goes together" })).toBeNull();
    expect(screen.getByRole("switch", { name: "Your calendar today" })).toBeInTheDocument();
  });

  it("leaves out the page group when only a browser phone is paired", async () => {
    const engine = phonesEngine();
    engine.sim.fixtures.phones = engine.sim.fixtures.phones.filter((p) => p.inBrowser);
    await openPhones(PATH, "From your phones", engine);
    expect(screen.queryByText("On your page")).toBeNull();
    expect(screen.queryByRole("switch")).toBeNull();
  });

  it("says so when no phone has paired", async () => {
    await openPhones(PATH, "From your phones", phonesEngine({ empty: true }));
    expect(
      screen.getByText("No phone has brought its calendar or health yet."),
    ).toBeInTheDocument();
  });
});
