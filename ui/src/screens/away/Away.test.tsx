import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { chipsIn, sentence } from "../../test/sentence";
import { Confirm } from "./parts";
import { openAway } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

describe("away", () => {
  it("off_never_set_up: says it is off and offers the setup", async () => {
    await openAway("/settings/away");
    expect(screen.getByText("Off", { selector: "span" })).toBeInTheDocument();
    expect(
      screen.getAllByRole("link", { name: "Settings" }).map((a) => a.className),
    ).toContainEqual(expect.stringContaining("back"));
    expect(
      screen.getByText(/Nothing is downloaded, started or connected until you set it up/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set up…" })).toBeInTheDocument();
    expect(screen.getByText("Your phone can’t")).toBeInTheDocument();
    expect(
      screen.getByText("Things you marked private stay on this computer."),
    ).toBeInTheDocument();
  });

  it("on_simple_two_phones: shows the switch, the connection, the phones and what was sent", async () => {
    await openAway("/settings/away", "on");
    expect(screen.getByRole("switch", { name: "Away from home is on" })).toBeChecked();
    expect(screen.getByText("Simple")).toBeInTheDocument();
    expect(screen.queryByText("Trial")).toBeNull();
    expect(screen.getByText("Phones · 2")).toBeInTheDocument();
    expect(screen.getByText("Work phone")).toBeInTheDocument();
    expect(screen.getByText("Last used yesterday, 18:40")).toBeInTheDocument();
    expect(screen.getAllByText("3.2 MB").length).toBeGreaterThan(0);
    expect(screen.getByText("Counted here, as it leaves this computer.")).toBeInTheDocument();
    expect(screen.getByText("Never", { selector: "span" })).toBeInTheDocument();
  });

  it("relink_lists_each_phone: shows the code, the four words and who has the new address", async () => {
    await openAway("/settings/away", "relink");
    expect(
      screen.getByRole("heading", {
        name: "Your computer has a new address. Scan this code with each phone to reconnect.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("river · copper · meadow · thistle")).toBeInTheDocument();
    const how = screen.getByText(
      sentence(
        "On each phone, open Nocturne and choose Scan the new code, then point it at your computer. Your phones are still paired, so there’s nothing to allow.",
      ),
    );
    expect(chipsIn(how)).toEqual(["Scan the new code"]);
    const list = screen.getByRole("list", { name: "Phones and whether they have the new address" });
    expect(within(list).getByText(/^Reconnected at \d\d:\d\d$/)).toBeInTheDocument();
    expect(within(list).getByText("Not yet")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /new address/ })).toBeInTheDocument();
  });

  it("problem_offers_retry_and_switch: says what failed and offers a way on", async () => {
    const { engine } = await openAway("/settings/away", "problem");
    expect(screen.getByText("Couldn’t start")).toBeInTheDocument();
    expect(screen.getByText(/Cloudflare didn’t answer/)).toBeInTheDocument();
    expect(screen.getByText(/tried 3 times since/)).toBeInTheDocument();
    expect(screen.getByText(/Anything your phones keep waits on them/)).toBeInTheDocument();
    expect(screen.getByText(/Still paired/)).toBeInTheDocument();
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(start).toHaveBeenCalledWith("simple", false);
    expect(await screen.findByText(/Nocturne is starting the connection/)).toBeInTheDocument();
  });

  it("problem_offers_retry_and_switch: switching warns before it unpairs", async () => {
    const { engine } = await openAway("/settings/away", "problem");
    await userEvent.click(screen.getByRole("button", { name: "Switch to Most private…" }));
    const dialog = await screen.findByRole("dialog", { name: "Switch to Most private?" });
    expect(within(dialog).getByText(/Your phones will need to be added again/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Switch and unpair phones" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away/setup"));
    expect((await engine.getRemote()).devices).toEqual([]);
  });

  it("change_connection_warns_and_unpairs: names the cost, then removes every phone", async () => {
    const { engine } = await openAway("/settings/away", "on");
    expect(
      screen.getAllByText(
        /Switching between Simple and Most private means pairing your phones again/,
      ).length,
    ).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole("button", { name: "Change…" }));
    const dialog = await screen.findByRole("dialog", { name: "Switch to Most private?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep Simple" }));
    expect((await engine.getRemote()).devices).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Change…" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Switch and unpair phones",
      }),
    );
    await waitFor(async () => expect((await engine.getRemote()).devices).toEqual([]));
    expect((await engine.getRemote()).connection).toBe("mostPrivate");
  });

  it("turn_off_and_remove_all_confirms: asks first, then turns off and removes the phones", async () => {
    const { engine } = await openAway("/settings/away", "on");
    await userEvent.click(screen.getByRole("button", { name: "Turn off and remove all phones…" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn off and remove all phones?" });
    expect(within(dialog).getByText(/Nothing on this computer is deleted/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Turn off and remove" })).toHaveAttribute(
      "data-variant",
      "forget",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep it on" }));
    expect((await engine.getRemote()).state.type).toBe("on");
    await userEvent.click(screen.getByRole("button", { name: "Turn off and remove all phones…" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Turn off and remove",
      }),
    );
    await waitFor(async () => expect((await engine.getRemote()).state.type).toBe("off"));
    expect((await engine.getRemote()).devices).toEqual([]);
  });

  it("turning the switch off keeps the phones paired", async () => {
    const { engine } = await openAway("/settings/away", "on");
    await userEvent.click(screen.getByRole("switch", { name: "Away from home is on" }));
    await waitFor(async () => expect((await engine.getRemote()).state.type).toBe("off"));
    expect((await engine.getRemote()).devices).toHaveLength(2);
    expect(await screen.findByText(/Your 2 phones stay paired/)).toBeInTheDocument();
  });

  it("auto_off_select_saves: saves the choice and says so", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const select = screen.getByRole("combobox", { name: "Turn off by itself" });
    expect(select).toHaveDisplayValue("When no phone has used it for 30 days");
    await userEvent.selectOptions(select, "After a week");
    await waitFor(async () => expect((await engine.getRemote()).autoOff).toBe("afterWeek"));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
  });

  it("keep_awake_says_plugged_in: the row says it only holds while plugged in", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const toggle = screen.getByRole("switch", { name: "Keep this computer awake while it’s on" });
    expect(toggle).toHaveAccessibleDescription(/Only while plugged in/);
    await userEvent.click(toggle);
    await waitFor(async () => expect((await engine.getRemote()).keepAwake).toBe(true));
  });

  it("no_phone_code_row: there is no phone code setting", async () => {
    await openAway("/settings/away", "on");
    expect(screen.queryByText(/phone code/i)).toBeNull();
    expect(screen.queryByText(/12 hours/)).toBeNull();
  });

  it("advanced_only_place_with_jargon: the technical words appear only after asking for them", async () => {
    await openAway("/settings/away", "on");
    const page = document.body.textContent ?? "";
    expect(page).not.toMatch(/tunnel|HKDF|AES|sha256|helper/i);
    await userEvent.click(screen.getByRole("button", { name: "Show advanced details" }));
    const region = await screen.findByRole("region", { name: "Advanced details" });
    expect(region).toHaveTextContent("Simple · Cloudflare quick tunnel · trial");
    expect(region).toHaveTextContent(/cloudflared 2026\.10\.1 · sha256 checked/);
    expect(region).toHaveTextContent("unix socket, owner only");
    expect(screen.getByRole("button", { name: "Hide advanced details" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("advanced_lists_what_is_not_hidden: says what the middle still sees and the one caveat", async () => {
    await openAway("/settings/away", "on");
    await userEvent.click(screen.getByRole("button", { name: "Show advanced details" }));
    const region = await screen.findByRole("region", { name: "Advanced details" });
    expect(within(region).getByText("What the channel does not hide")).toBeInTheDocument();
    expect(region).toHaveTextContent(/stable id for each phone/);
    expect(region).toHaveTextContent(/Cloudflare serves the phone page’s code/);
    expect(region).toHaveTextContent(/until the phone is added again/);
    expect(region).toHaveTextContent(/shows up in public certificate logs/);
    expect(region).not.toHaveTextContent(/Most private avoids this/);
    expect(region).toHaveTextContent(/look-alike site that has the typed code/);
    expect(region).toHaveTextContent(/stopped only by comparing the three words/);
    expect(region).toHaveTextContent(
      /unlocked or stolen phone can read everything not marked private/,
    );
    expect(region).toHaveTextContent(/any person using this computer can see the public address/);
    expect(region).toHaveTextContent(/No sealed removal notice is possible/);
  });

  it("a new computer key asks first and unpairs every phone", async () => {
    const { engine } = await openAway("/settings/away", "on");
    await userEvent.click(screen.getByRole("button", { name: "Show advanced details" }));
    await userEvent.click(await screen.findByRole("button", { name: "Make a new computer key…" }));
    const dialog = await screen.findByRole("dialog", { name: "Make a new computer key?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Make a new key" }));
    await waitFor(() => expect(engine.remote.keysMade).toBe(1));
    expect((await engine.getRemote()).devices).toEqual([]);
  });

  it("announces a state change as it happens", async () => {
    const { engine } = await openAway("/settings/away", "on");
    act(() => engine.remote.restartSimple());
    expect(
      await screen.findByRole("heading", { name: /Your computer has a new address/ }),
    ).toBeInTheDocument();
    const live = document.querySelector("[aria-live=polite]");
    expect(live).toHaveTextContent("Your phones need the new address");
  });

  it("an event that arrives before a command's reply is not overwritten by the reply", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const stale = engine.remote.view();
    vi.spyOn(engine, "updateRemoteSettings").mockImplementation(async () => {
      engine.remote.restartSimple();
      return stale;
    });
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Turn off by itself" }),
      "After a day",
    );
    expect(
      await screen.findByRole("heading", { name: /Your computer has a new address/ }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("switch", { name: "Away from home is on" })).toBeNull(),
    );
  });

  it("double clicks: confirming a switch or removing everything is sent once", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const remove = vi.spyOn(engine, "stopRemote");
    await userEvent.click(screen.getByRole("button", { name: "Turn off and remove all phones…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.dblClick(within(dialog).getByRole("button", { name: "Turn off and remove" }));
    expect(remove).toHaveBeenCalledTimes(1);
  });

  it("double clicks: a new computer key is made once", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const make = vi.spyOn(engine, "newComputerKey");
    await userEvent.click(screen.getByRole("button", { name: "Show advanced details" }));
    await userEvent.click(await screen.findByRole("button", { name: "Make a new computer key…" }));
    await userEvent.dblClick(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Make a new key" }),
    );
    expect(make).toHaveBeenCalledTimes(1);
  });

  it("double clicks: switching connection is sent once", async () => {
    const { engine } = await openAway("/settings/away", "on");
    const update = vi.spyOn(engine, "updateRemoteSettings");
    await userEvent.click(screen.getByRole("button", { name: "Change…" }));
    await userEvent.dblClick(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Switch and unpair phones",
      }),
    );
    expect(update).toHaveBeenCalledTimes(1);
  });

  it("moves focus to the new heading when the state changes, and says it is on", async () => {
    const { engine } = await openAway("/settings/away", "relink");
    act(() => engine.remote.phoneRelinks("work"));
    await screen.findByRole("switch", { name: "Away from home is on" });
    const heading = screen.getByRole("heading", { level: 1, name: "Away from home" });
    await waitFor(() => expect(heading).toHaveFocus());
    expect(await screen.findByText("Away from home is on.")).toHaveClass("sr-only");
  });

  it("names the company Turn on will use for someone who set it up before", async () => {
    await openAway("/settings/away", "offBefore");
    expect(screen.getByText("Turn on will use Simple, through Cloudflare.")).toBeInTheDocument();
    expect(screen.queryByText(/About two minutes/)).toBeNull();
  });
});

describe("Confirm", () => {
  it("fills the confirm button in ember only when the step is destructive", () => {
    const { rerender } = render(
      <Confirm title="Sure?" confirm="Do it" keep="Not now" onConfirm={() => {}} onClose={() => {}}>
        <p>Body</p>
      </Confirm>,
    );
    expect(screen.getByRole("button", { name: "Do it" })).toHaveAttribute(
      "data-variant",
      "primary",
    );
    rerender(
      <Confirm
        title="Sure?"
        confirm="Do it"
        keep="Not now"
        destructive
        onConfirm={() => {}}
        onClose={() => {}}
      >
        <p>Body</p>
      </Confirm>,
    );
    const button = screen.getByRole("button", { name: "Do it" });
    expect(button).toHaveAttribute("data-variant", "forget");
    expect(button).toHaveAttribute("data-filled", "true");
  });
});
