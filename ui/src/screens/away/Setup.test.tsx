import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { chipsIn, sentence } from "../../test/sentence";
import { mayAskAboutFirewall, mostPrivateAvailable } from "./os";
import { openAway, pretendComputer } from "./testing";

afterEach(() => {
  window.location.hash = "";
  pretendComputer("linux");
  vi.restoreAllMocks();
});

const SETUP = "/settings/away/setup";
const STEP1 = /Use Nocturne from your phone when you’re out/;

describe("setup", () => {
  it("simple_is_preselected_with_trial: offers both ways, Simple first, no Trial badge", async () => {
    await openAway(SETUP, "off", STEP1);
    expect(screen.getByRole("radio", { name: /Simple/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /Most private/ })).not.toBeChecked();
    expect(screen.queryByText("Trial")).toBeNull();
    expect(screen.getByText(/Nothing to sign up for/)).toBeInTheDocument();
    expect(screen.getByText(/A Tailscale account and its app on your phone/)).toBeInTheDocument();
    expect(screen.getByText("Away from home · step 1")).toBeInTheDocument();
  });

  it("light_first_step: each connection has one short line on who sees what", async () => {
    await openAway(SETUP, "off", STEP1);
    expect(
      screen.getByText(
        "Cloudflare carries locked messages between your phone and this computer. It can see when your phone connects and from where, and it supplies the page your phone opens.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Usually your phone and computer talk to each other directly. Tailscale can see your account, the devices on it and when they connect, but not what you keep or ask.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Cloudflare offers this for free without promises/),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Can’t delete, export or change settings. Do those here."),
    ).toBeVisible();
    expect(screen.getByText("Stay on this computer and never reach your phone.")).toBeVisible();
    expect(screen.queryByText(/Nothing goes anywhere else/)).toBeNull();
    expect(screen.queryByText(/While this is on, your phone can reach this computer/)).toBeNull();
    expect(screen.queryByText(/It also serves the page your phone opens/)).toBeNull();
    expect(screen.queryByText("Your phone can")).toBeNull();
    expect(screen.queryByText(/Switching between Simple and Most private/)).toBeNull();
  });

  it("light_first_step: one expander holds the full, corrected detail", async () => {
    await openAway(SETUP, "off", STEP1);
    const toggle = screen.getByRole("button", {
      name: "Who sees what, and what your phone can do",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).not.toHaveAttribute("aria-controls");
    await userEvent.click(toggle);
    const panel = document.getElementById(
      toggle.getAttribute("aria-controls") ?? "",
    ) as HTMLElement;
    const inside = within(panel);
    const labels = Array.from(panel.querySelectorAll("dt")).map((dt) => dt.textContent);
    expect(labels).toEqual(["Simple", "Most private", "Either way"]);
    const simple = panel.querySelector("dd") as HTMLElement;
    expect(simple).toHaveTextContent("a number that tells your phones apart");
    expect(simple).toHaveTextContent("and could act as your phone while the page is open");
    expect(simple).toHaveTextContent(
      "After this computer restarts, your phone looks for it at its old address, which Cloudflare, or whoever gets that address later, may answer.",
    );
    expect(simple).toHaveTextContent("or one made by someone who broke into it");
    const private_ = panel.querySelectorAll("dd")[1] as HTMLElement;
    expect(private_).toHaveTextContent(
      "It can see your account, the devices on it and when they connect",
    );
    expect(private_).toHaveTextContent("each still needs your Allow to pair.");
    expect(chipsIn(private_.querySelector("p") as HTMLElement)).toEqual(["Allow"]);
    expect(
      inside.getByText(
        "While it’s on, the connection part also talks to its company’s servers to stay connected.",
      ),
    ).toBeInTheDocument();
    for (const line of [
      "Ask, with where each answer came from",
      "Keep a note, a photo or a voice note",
      "Read today’s page and the last week’s",
      "Delete anything",
      "Export what you’ve kept",
      "Change settings or add phones",
      "Do those here, on this computer.",
      "Anyone holding your unlocked phone can do what it can. If a phone is lost, remove it here and it stops working at once.",
      "Switching between Simple and Most private means pairing your phones again.",
      "Leaving at any step turns Away from home back off.",
    ]) {
      expect(inside.getByText(line), line).toBeInTheDocument();
    }
  });

  it("light_first_step: the bar says nothing is downloaded or turned on until the next step", async () => {
    await openAway(SETUP, "off", STEP1);
    const bar = document.querySelector("[data-action-bar]") as HTMLElement;
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Continue", "Not now"]);
    expect(
      within(bar).getByText("Nothing is downloaded or turned on until the next step."),
    ).toBeVisible();
  });

  it("download_needs_the_click: nothing downloads until Download and turn on is pressed", async () => {
    const { engine } = await openAway(SETUP, "off", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", {
        name: /small extra part from Cloudflare \(about 40\sMB\)/,
      }),
    ).toBeInTheDocument();
    expect(start).not.toHaveBeenCalled();
    const body = screen.getByText(
      sentence(
        "It’s the piece that lets your phone reach this computer through Cloudflare. Nocturne checks it before it’s used, and the download shows in Privacy, like every other.",
      ),
    );
    expect(chipsIn(body)).toEqual(["Privacy"]);
    expect(screen.getByText("Only while Away from home is on")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Download and turn on" }));
    expect(start).toHaveBeenCalledWith("simple", true);
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();
  });

  it("step_two_shows_everything: the full detail is on screen before anything is turned on", async () => {
    const { engine } = await openAway(SETUP, "off", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: /small extra part from Cloudflare/ });
    expect(
      screen.getByText(
        "While this is on, your phone can reach this computer over the internet, through Cloudflare.",
      ),
    ).toBeInTheDocument();
    const sees = screen.getByRole("heading", { name: "Who sees what" })
      .parentElement as HTMLElement;
    expect(sees).toHaveTextContent("a number that tells your phones apart");
    expect(sees).toHaveTextContent("could act as your phone while the page is open");
    expect(sees).toHaveTextContent("your phone looks for it at its old address");
    expect(sees).toHaveTextContent(
      "While it’s on, the connection part also talks to its company’s servers to stay connected.",
    );
    const phone = screen.getByRole("heading", { name: "Your phone" }).parentElement as HTMLElement;
    expect(phone).toHaveTextContent(
      "Your phone can ask, keep a note, a photo or a voice note, and read today’s page and the last week’s. It can’t delete anything, export what you’ve kept, change settings or add phones. Things you marked private never reach it.",
    );
    expect(phone).toHaveTextContent("Anyone holding your unlocked phone can do what it can.");
    const bar = document.querySelector("[data-action-bar]") as HTMLElement;
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Download and turn on", "Not now"]);
    expect(start).not.toHaveBeenCalled();
  });

  it("step_two_first: a part that is already here still asks on step 2 before turning on", async () => {
    const { engine } = await openAway(SETUP, "offBefore", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", { name: "Turn on Away from home through Cloudflare?" }),
    ).toBeInTheDocument();
    expect(start).not.toHaveBeenCalled();
    expect(screen.queryByText("Only while Away from home is on")).toBeNull();
    expect(screen.queryByRole("button", { name: /Download/ })).toBeNull();
    const bar = document.querySelector("[data-action-bar]") as HTMLElement;
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Turn on", "Not now"]);
    await userEvent.click(within(bar).getByRole("button", { name: "Turn on" }));
    expect(start).toHaveBeenCalledWith("simple", false);
  });

  it("step_two_for_most_private: names Tailscale and its own detail", async () => {
    await openAway(SETUP, "off", STEP1);
    await userEvent.click(screen.getByRole("radio", { name: /Most private/ }));
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: /small extra part from Tailscale/ });
    const sees = screen.getByRole("heading", { name: "Who sees what" })
      .parentElement as HTMLElement;
    expect(sees).toHaveTextContent(
      "It can see your account, the devices on it and when they connect",
    );
    expect(sees).not.toHaveTextContent("Cloudflare");
  });

  it("download_needs_the_click: Not now starts nothing", async () => {
    const { engine } = await openAway(SETUP, "off", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "Not now" }));
    expect(start).not.toHaveBeenCalled();
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away"));
  });

  it("pause_and_cancel: pauses and resumes the part, and cancelling turns it off", async () => {
    const { engine } = await openAway(
      SETUP,
      "downloading",
      /Getting the extra part from Cloudflare/,
    );
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute(
      "aria-valuetext",
      expect.stringContaining("18\u00a0MB of 40\u00a0MB"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(await screen.findByRole("button", { name: "Resume" })).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuetext",
      expect.stringContaining("Paused"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
    const stop = vi.spyOn(engine, "stopRemote");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(stop).toHaveBeenCalledWith(false);
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away"));
  });

  it("no_internet_says_so: says the computer cannot reach the internet and offers another go", async () => {
    await openAway(SETUP, "noInternet");
    expect(screen.getByText(/can’t reach the internet/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("sign_in_waits_and_moves_on: opens the page, waits, then carries on by itself", async () => {
    const { engine } = await openAway(SETUP, "signIn", /Sign in to Tailscale in your browser/);
    const open = vi.spyOn(engine, "openRemoteSignIn");
    expect(screen.getByText("Waiting for you to sign in…")).toBeInTheDocument();
    expect(screen.getByText(/step 3 of 6 · Most private/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open the page again" }));
    expect(open).toHaveBeenCalledOnce();
    expect(engine.remote.signInOpened).toBeGreaterThanOrEqual(2);
    act(() => engine.remote.advance());
    expect(
      await screen.findByRole("heading", { name: "Allow secure links in Tailscale." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/find HTTPS Certificates and choose Enable HTTPS/)).toBeInTheDocument();
    act(() => engine.remote.advance());
    expect(
      await screen.findByRole("heading", { name: "Get Tailscale on your phone." }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away/add?setup=1"));
  });

  it("approve_step: asks the owner to approve this computer and opens the page once", async () => {
    const { engine } = await openAway(SETUP, "approve", /Approve this computer in Tailscale/);
    expect(screen.getByText(/step 3 of 6 · Most private/)).toBeInTheDocument();
    expect(screen.getByText(/Waiting for the approval/)).toBeInTheDocument();
    expect(engine.remote.signInOpened).toBe(1);
    expect(screen.getByRole("button", { name: "Copy the link" })).toBeInTheDocument();
  });

  it("Simple goes straight to adding a phone once it is on", async () => {
    const { engine } = await openAway(SETUP, "starting", /Starting the connection/);
    act(() => engine.remote.advance());
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away/add?setup=1"));
  });

  it("firewall_line_on_mac_and_windows_only: only a computer that may ask is told", async () => {
    for (const kind of ["mac", "linux"] as const) {
      pretendComputer(kind);
      const { unmount } = await openAway(SETUP, "off", STEP1);
      await userEvent.click(screen.getByRole("radio", { name: /Most private/ }));
      await userEvent.click(screen.getByRole("button", { name: "Continue" }));
      await screen.findByRole("heading", { name: /small extra part from Tailscale/ });
      const line = screen.queryByText(
        /may ask whether Nocturne’s Tailscale part may accept connections/,
      );
      expect(Boolean(line), kind).toBe(kind === "mac");
      unmount();
    }
    expect(mayAskAboutFirewall("windows")).toBe(true);
    expect(mayAskAboutFirewall("mac")).toBe(true);
    expect(mayAskAboutFirewall("linux")).toBe(false);
  });

  it("leaving_turns_off: leaving the setup before a phone is added stops it", async () => {
    const { engine } = await openAway(SETUP, "downloading", /Getting the extra part/);
    const stop = vi.spyOn(engine, "stopRemote");
    act(() => {
      window.location.hash = "#/privacy";
    });
    await waitFor(() => expect(stop).toHaveBeenCalledWith(false));
    expect((await engine.getRemote()).state.type).toBe("off");
  });

  it("most_private_hidden_when_unsupported: a computer that cannot run it is only offered Simple", async () => {
    pretendComputer("windows");
    expect(mostPrivateAvailable()).toBe(false);
    await openAway(SETUP, "off", STEP1);
    expect(screen.getByRole("radio", { name: /Simple/ })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Most private/ })).toBeNull();
  });

  it("double clicks: Download and turn on sends one start however many times it is pressed", async () => {
    const { engine } = await openAway(SETUP, "off", STEP1);
    let release: () => void = () => undefined;
    const real = engine.startRemote.bind(engine);
    const start = vi.spyOn(engine, "startRemote").mockImplementation(
      (c, d) =>
        new Promise((resolve) => {
          release = () => resolve(real(c, d));
        }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.dblClick(await screen.findByRole("button", { name: "Download and turn on" }));
    expect(start).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Download and turn on" })).toBeDisabled();
    release();
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();
  });

  it("double clicks: Pause is sent once", async () => {
    const { engine } = await openAway(SETUP, "downloading", /Getting the extra part/);
    let release: () => void = () => undefined;
    const pause = vi.spyOn(engine, "pauseRemoteDownload").mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () => resolve(engine.remote.view());
        }),
    );
    await userEvent.dblClick(screen.getByRole("button", { name: "Pause" }));
    expect(pause).toHaveBeenCalledTimes(1);
    release();
  });

  it("leaving during a start: a start still in flight is stopped when the person leaves", async () => {
    const { engine } = await openAway(SETUP, "offBefore", STEP1);
    vi.spyOn(engine, "startRemote").mockImplementation(() => new Promise(() => undefined));
    const stop = vi.spyOn(engine, "stopRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "Turn on" }));
    act(() => {
      window.location.hash = "#/privacy";
    });
    await waitFor(() => expect(stop).toHaveBeenCalledWith(false));
  });

  it("starting can be cancelled", async () => {
    const { engine } = await openAway(SETUP, "starting", /Starting the connection/);
    const stop = vi.spyOn(engine, "stopRemote");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(stop).toHaveBeenCalledWith(false);
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away"));
  });

  it("choosing another connection while phones are paired asks before unpairing them", async () => {
    const { engine } = await openAway(SETUP, "offBefore", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("radio", { name: /Most private/ }));
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    const dialog = await screen.findByRole("dialog", { name: "Switch to Most private?" });
    expect(dialog).toHaveTextContent(/Your phones will need to be added again/);
    expect(start).not.toHaveBeenCalled();
    expect((await engine.getRemote()).devices).toHaveLength(2);
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep Simple" }));
    expect((await engine.getRemote()).devices).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", {
        name: "Switch and unpair phones",
      }),
    );
    await waitFor(async () => expect((await engine.getRemote()).devices).toEqual([]));
    expect(window.location.hash).toBe("#/settings/away/setup");
  });
});
