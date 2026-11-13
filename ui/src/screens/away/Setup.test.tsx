import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
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
    expect(screen.getByText(/It also serves the page your phone opens/)).toBeInTheDocument();
    expect(screen.getByText("Away from home · step 1")).toBeInTheDocument();
  });

  it("download_needs_the_click: nothing downloads until Download is pressed", async () => {
    const { engine } = await openAway(SETUP, "off", STEP1);
    const start = vi.spyOn(engine, "startRemote");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", {
        name: /small extra part from Cloudflare \(about 40 MB\)/,
      }),
    ).toBeInTheDocument();
    expect(start).not.toHaveBeenCalled();
    expect(screen.getByText("Only while Away from home is on")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(start).toHaveBeenCalledWith("simple", true);
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();
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
    expect(bar).toHaveAttribute("aria-valuetext", expect.stringContaining("18 MB of 40 MB"));
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

  it("double clicks: Download sends one start however many times it is pressed", async () => {
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
    await userEvent.dblClick(await screen.findByRole("button", { name: "Download" }));
    expect(start).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Download" })).toBeDisabled();
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
