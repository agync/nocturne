import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  window.history.replaceState({}, "", "/");
  document.documentElement.removeAttribute("data-theme");
  vi.restoreAllMocks();
});

/** An engine that starts in one of the states of getting ready. */
const at = (name: string, options: Parameters<typeof createMockEngine>[0] = {}) => {
  window.history.replaceState({}, "", `/?mockSetup=${name}`);
  const engine = createMockEngine(options);
  window.history.replaceState({}, "", "/");
  return engine;
};

const open = async (
  name: string,
  hash = "#/setup",
  platform = fakePlatform(),
  options: Parameters<typeof createMockEngine>[0] = {},
) => {
  const engine = at(name, options);
  renderApp({ hash, engine, platform });
  await screen.findByRole("main", { name: "Setting up Nocturne" });
  return { engine, platform };
};

describe("the welcome", () => {
  it("is the night screen, with the promise, and Get started moves on to checking", async () => {
    await open("notStarted");
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Keep anything. Nocturne sleeps on it and hands you one page each morning.",
      }),
    ).toBeInTheDocument();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(screen.getByText("Everything stays on this computer.")).toBeInTheDocument();
    expect(screen.getByText(/No account. No cloud./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Get started" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Checking this computer…" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/computer memory and free space/).closest("[role=status]"),
    ).not.toBeNull();
    expect(document.documentElement.getAttribute("data-theme")).toBeNull();
  });

  it("can be opened again from a finished setup", async () => {
    await open("done", "#/setup?step=welcome");
    await userEvent.click(await screen.findByRole("button", { name: "Get started" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: /lighter mind/ }),
    ).toBeInTheDocument();
  });
});

describe("what this computer can run", () => {
  it("shows the four tiers, the detected one chosen, and what it needs", async () => {
    await open("choosing", "#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Nocturne will use a lighter mind. Everything works; tidying takes a little longer.",
      }),
    ).toBeInTheDocument();
    const group = screen.getByRole("group", { name: "How much to set up" });
    const radios = within(group).getAllByRole("radio");
    expect(radios.map((r) => (r as HTMLInputElement).value)).toEqual([
      "large",
      "standard",
      "small",
      "tiny",
    ]);
    expect(within(group).getByRole("radio", { name: /Standard · this computer/ })).toBeChecked();
    expect(within(group).getByRole("radio", { name: /Large/ })).toBeDisabled();
    expect(within(group).getByText("Too big for this computer’s memory")).toBeInTheDocument();
    expect(
      screen.getByText(/Nocturne needs 8\.4 GB of space \(you have 220 GB free\)/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set up · 8.4 GB" })).toBeInTheDocument();
  });

  it("says what each size is for and what it costs in words that read on their own", async () => {
    await open("choosing", "#/setup?step=choosing");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    expect(
      within(group).getByText("For computers with 28 GB memory or more · 8.4 GB download"),
    ).toBeInTheDocument();
    expect(
      within(group).getByText(/· 4\.8 GB download · slower · uses less memory, space and battery$/),
    ).toBeInTheDocument();
    expect(
      within(group).getByText(
        "For computers with 7 GB memory or more · 3.2 GB download · uses the least memory, space and battery · answers are simpler and may miss details",
      ),
    ).toBeInTheDocument();
  });

  it("lets a powerful computer choose the tiny mind", async () => {
    const { engine } = await open("large", "#/setup?step=choosing");
    const spy = vi.spyOn(engine, "startSetup");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    expect(within(group).getByRole("radio", { name: /Large · this computer/ })).toBeChecked();
    const tiny = within(group).getByRole("radio", { name: /Tiny/ });
    expect(tiny).toBeEnabled();
    await userEvent.click(tiny);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "Nocturne will use its lightest mind. Everything works, but answers are simpler and it may miss details.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Set up · 3.2 GB" }));
    expect(spy).toHaveBeenCalledWith("tiny");
  });

  it("offers only the tiny mind on an 8 GB computer, already chosen", async () => {
    await open("tiny", "#/setup?step=choosing");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    const enabled = within(group)
      .getAllByRole("radio")
      .filter((r) => !(r as HTMLInputElement).disabled)
      .map((r) => (r as HTMLInputElement).value);
    expect(enabled).toEqual(["tiny"]);
    expect(within(group).getByRole("radio", { name: /Tiny · this computer/ })).toBeChecked();
  });

  it("chooses Small on a 16 GB computer and still allows Tiny", async () => {
    await open("small", "#/setup?step=choosing");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    expect(within(group).getByRole("radio", { name: /Small · this computer/ })).toBeChecked();
    expect(within(group).getByRole("radio", { name: /Tiny/ })).toBeEnabled();
    expect(within(group).getByRole("radio", { name: /Standard/ })).toBeDisabled();
  });

  it("says when a size needs more free space rather than more memory", async () => {
    await open("shortOfSpace", "#/setup?step=choosing");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    expect(within(group).getByRole("radio", { name: /Large/ })).toBeDisabled();
    expect(within(group).getAllByText("Too big for the free space")).toHaveLength(2);
    expect(within(group).queryByText("Too big for this computer’s memory")).toBeNull();
    expect(within(group).getByRole("radio", { name: /Small · this computer/ })).toBeChecked();
  });

  it("puts focus on the new step's heading after Get started and after Set up", async () => {
    await open("choosing", "#/setup");
    await userEvent.click(await screen.findByRole("button", { name: "Get started" }));
    const choosing = await screen.findByRole("heading", { level: 1, name: /lighter mind/ });
    await waitFor(() => expect(choosing).toHaveFocus());
    await userEvent.click(screen.getByRole("button", { name: /^Set up · / }));
    const ready = await screen.findByRole("heading", {
      level: 1,
      name: "While Nocturne gets ready, keep your first thing.",
    });
    await waitFor(() => expect(ready).toHaveFocus());
  });

  it("titles the window", async () => {
    await open("notStarted");
    expect(document.title).toBe("Getting ready · Nocturne");
  });

  it("changes the headline and the button when another tier is chosen", async () => {
    await open("choosing", "#/setup?step=choosing");
    await userEvent.click(await screen.findByRole("radio", { name: /Small/ }));
    expect(screen.getByRole("button", { name: "Set up · 4.8 GB" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/smallest mind/);
  });

  it("says the best mind for a powerful computer", async () => {
    await open("large", "#/setup?step=choosing");
    expect(await screen.findByRole("heading", { level: 1, name: /best mind/ })).toBeInTheDocument();
  });

  it("explains Why this one? in plain words from the tier table", async () => {
    await open("choosing", "#/setup?step=choosing");
    const why = await screen.findByRole("button", { name: "Why this one?" });
    expect(why).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(why);
    expect(why).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByText(
        /32 GB here. Large needs 60 GB or more, Standard 28 GB, Small 14 GB and Tiny 7 GB/,
      ),
    ).toBeInTheDocument();
  });

  it("starts setting up with the chosen tier and moves to getting ready", async () => {
    const { engine } = await open("choosing", "#/setup?step=choosing");
    const spy = vi.spyOn(engine, "startSetup");
    await userEvent.click(await screen.findByRole("button", { name: "Set up · 8.4 GB" }));
    expect(spy).toHaveBeenCalledWith("standard");
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "While Nocturne gets ready, keep your first thing.",
      }),
    ).toBeInTheDocument();
  });

  it("says when there is not enough memory, with no button to set up", async () => {
    await open("unsupported", "#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "There isn’t enough computer memory for Nocturne yet.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "This computer has 6 GB of computer memory. Nocturne needs 7 GB or more. Nothing has been downloaded.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Set up/ })).toBeNull();
  });

  it("says when there is not enough space, and offers to check again", async () => {
    await open("lowDisk", "#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", { level: 1, name: /needs more free space/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/It has 3 GB free\. Nocturne needs 5\.2 GB to set up/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check again" })).toBeInTheDocument();
  });
});

describe("getting ready", () => {
  it("shows progress in words, how to pause, and that closing is safe", async () => {
    const { engine } = await open("downloading");
    const bar = await screen.findByRole("progressbar", { name: "Setting up Nocturne’s mind" });
    expect(bar).toHaveAttribute(
      "aria-valuetext",
      "Setting up Nocturne’s mind, 4.9 GB of 8.4 GB · about 17 min",
    );
    expect(
      screen.getByText("Safe to close. It carries on next time, even after losing Wi-Fi."),
    ).toBeInTheDocument();
    const spy = vi.spyOn(engine, "pauseSetup");
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(spy).toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: "Resume" })).toBeInTheDocument();
  });

  it("says Paused when paused, and Resume carries on", async () => {
    const { engine } = await open("paused");
    expect(await screen.findByText("Paused. Nothing is lost.")).toBeInTheDocument();
    const spy = vi.spyOn(engine, "resumeSetup");
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(spy).toHaveBeenCalled();
  });

  it("says when it is waiting for the internet (E-01), with Try now", async () => {
    const { engine } = await open("offline");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The download stopped when the internet dropped. It will carry on by itself.",
    );
    const spy = vi.spyOn(engine, "resumeSetup");
    await userEvent.click(screen.getByRole("button", { name: "Try now" }));
    expect(spy).toHaveBeenCalled();
  });

  it("says how much more space is needed (E-02)", async () => {
    await open("diskFull");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This computer needs 6 GB more free space to finish setting up.",
    );
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("explains a battery pause", async () => {
    await open("battery");
    expect(
      await screen.findByText("Paused while on battery. It carries on when plugged in."),
    ).toBeInTheDocument();
  });

  it("has a state for installing", async () => {
    await open("installing");
    expect(await screen.findByRole("progressbar", { name: "Finishing up" })).toBeInTheDocument();
    expect(screen.getByText(/putting the last pieces in place/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
  });

  it("has a state for failed, and Try again starts it again", async () => {
    const { engine } = await open("failed");
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn’t finish getting ready");
    const spy = vi.spyOn(engine, "startSetup");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(spy).toHaveBeenCalledWith("standard");
  });

  it("offers Try again for a failed setup even when the engine gave no reason", async () => {
    const { engine } = await open("failedQuietly");
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn’t finish getting ready");
    const spy = vi.spyOn(engine, "startSetup");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(spy).toHaveBeenCalledWith("standard");
  });

  it("says the chosen size is too big for this computer and leads to choosing a smaller one", async () => {
    await open("tooBig");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "There isn’t enough computer memory for the size you chose.",
    );
    expect(screen.queryByText(/Nothing has been downloaded/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Choose a smaller size" }));
    expect(window.location.hash).toBe("#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", { level: 1, name: /lighter mind/ }),
    ).toBeInTheDocument();
  });

  it("lets the person keep a file, a note or a voice note straight away", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFiles").mockResolvedValue(["letter.jpg"]);
    const { engine } = await open("downloading", "#/setup", platform);
    const files = vi.spyOn(engine, "keepFiles");
    const text = vi.spyOn(engine, "keepText");
    await userEvent.click(await screen.findByRole("button", { name: "Choose a file" }));
    expect(files).toHaveBeenCalledWith(["letter.jpg"], "drop");
    expect(screen.getByRole("button", { name: "Hold to talk" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Type a note" }));
    const note = screen.getByRole("textbox", { name: "Your note" });
    expect(note).toHaveFocus();
    await userEvent.type(note, "Ask Sam about Saturday");
    await userEvent.click(screen.getByRole("button", { name: "Keep it" }));
    expect(text).toHaveBeenCalledWith("Ask Sam about Saturday", "quickCapture");
    await waitFor(() => expect(screen.queryByRole("textbox", { name: "Your note" })).toBeNull());
  });
});

describe("good defaults", () => {
  it("shows the page time, shortcut and start with this computer, and no update checks", async () => {
    await open("downloading");
    const panel = await screen.findByRole("complementary", { name: /Good defaults/ });
    expect(within(panel).getByText("07:00")).toBeInTheDocument();
    expect(within(panel).getByText("Ctrl+Shift+Space")).toBeInTheDocument();
    expect(within(panel).getByRole("switch", { name: "Start with this computer" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(panel).queryByRole("switch", { name: /update/i })).toBeNull();
    expect(within(panel).queryByText(/update/i)).toBeNull();
  });

  it("says Tonight as the corrected copy, never promising a minute", async () => {
    await open("downloading");
    expect(
      await screen.findByText(/If the lid is closed, it catches up when you open it\./),
    ).toBeInTheDocument();
    expect(screen.queryByText(/under a minute/)).toBeNull();
  });

  it("changes the page time", async () => {
    const { engine } = await open("downloading");
    const spy = vi.spyOn(engine, "updateSettings");
    await userEvent.click(await screen.findByRole("button", { name: "Change page time" }));
    const select = screen.getByRole("combobox", { name: "Your page arrives at, time" });
    expect(select).toHaveFocus();
    await userEvent.selectOptions(select, "06:30");
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ pageTime: "06:30" }));
  });

  it("shows the privacy line, measured or not", async () => {
    await open("downloading", "#/setup", fakePlatform(), { sentMeasured: true });
    expect(await screen.findByText("Nothing")).toBeInTheDocument();
  });

  it("says it is not measured rather than claiming nothing was sent", async () => {
    await open("downloading");
    expect(await screen.findByText("Not measured on this computer")).toBeInTheDocument();
    expect(screen.queryByText("Nothing")).toBeNull();
  });
});

describe("done and the states around it", () => {
  it("says Nocturne is ready when setup is finished", async () => {
    await open("done");
    expect(
      await screen.findByRole("heading", { level: 1, name: "Nocturne is ready." }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open today’s page" })).toHaveAttribute(
      "href",
      "#/today",
    );
  });

  it("says so and offers to try again when it cannot be read", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "getSetup").mockRejectedValueOnce(new Error("no"));
    renderApp({ hash: "#/setup", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t read how setup is going.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Nocturne is ready." }),
    ).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
