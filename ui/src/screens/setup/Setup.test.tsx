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
/** A number is kept with its unit by a no-break space, so a name built from a size has one. */
const nb = (text: string) => text.replace(/(\d) (?=[kMG]?B\b)/g, "$1\u00a0");

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

describe("first open", () => {
  it("opens setup when it cannot tell", async () => {
    const engine = at("done");
    vi.spyOn(engine, "getSetup").mockRejectedValueOnce(new Error("no answer"));
    renderApp({ hash: "", engine, platform: fakePlatform() });
    await waitFor(() => expect(window.location.hash).toBe("#/setup"));
  });

  it.each(["notStarted", "checking", "choosing"])(
    "opens setup when no size was ever chosen (%s)",
    async (name) => {
      renderApp({ hash: "", engine: at(name), platform: fakePlatform() });
      await screen.findByRole("main", { name: "Setting up Nocturne" });
      expect(window.location.hash).toBe("#/setup");
    },
  );

  /** An engine in one of the states of getting ready, with nothing yet to read with. */
  const unready = (name: string) => {
    const engine = at(name);
    const status = engine.getStatus.bind(engine);
    vi.spyOn(engine, "getStatus").mockImplementation(async () => ({
      ...(await status()),
      state: "gettingReady",
    }));
    return engine;
  };

  it.each(["downloading", "paused", "engineDown"])(
    "opens setup again while the first size is not in place (%s)",
    async (name) => {
      renderApp({ hash: "", engine: unready(name), platform: fakePlatform() });
      await screen.findByRole("main", { name: "Setting up Nocturne" });
      expect(window.location.hash).toBe("#/setup");
    },
  );

  it("opens today's page while a change to another size downloads", async () => {
    renderApp({ hash: "", engine: at("downloading"), platform: fakePlatform() });
    await waitFor(() => expect(window.location.hash).toBe("#/today"));
  });

  it.each(["done", "failed"])(
    "opens today's page for someone already set up (%s)",
    async (name) => {
      renderApp({ hash: "", engine: at(name), platform: fakePlatform() });
      await waitFor(() => expect(window.location.hash).toBe("#/today"));
    },
  );
});

const PASS = "lantern orchard tuesday";
const BACKUP = "/backups/Nocturne backup 2026-11-15.nocturne";

/** Takes the welcome card's restore path to the card that says the memories are back. */
async function restoreInSetup(
  engine: ReturnType<typeof createMockEngine>,
  platform: ReturnType<typeof fakePlatform>,
) {
  engine.script.here = null;
  vi.spyOn(platform, "pickBackup").mockResolvedValue(BACKUP);
  await userEvent.click(await screen.findByRole("button", { name: "Restore from a backup" }));
  expect(
    await screen.findByRole("heading", { level: 1, name: "Restore from a backup" }),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Choose backup file…" }));
  await userEvent.type(await screen.findByLabelText("Passphrase for this backup"), PASS);
  await userEvent.click(screen.getByRole("button", { name: "Open backup" }));
  await screen.findByRole("heading", { level: 1, name: "Ready to restore" });
  await userEvent.click(screen.getByRole("button", { name: "Restore" }));
  expect(
    await screen.findByRole("heading", { level: 1, name: "Your memories are back." }),
  ).toBeInTheDocument();
}

describe("night", () => {
  it.each([
    ["checking", "#/setup?step=choosing"],
    ["choosing", "#/setup?step=choosing"],
    ["unsupported", "#/setup?step=choosing"],
    ["downloading", "#/setup"],
    ["diskFull", "#/setup"],
    ["done", "#/setup"],
  ])("stays dark on the %s step, whatever the system theme is", async (name, hash) => {
    await open(name, hash, fakePlatform("light"));
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("dark"));
    await new Promise((r) => setTimeout(r, 20));
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("gives the normal theme back after setup", async () => {
    await open("done", "#/setup", fakePlatform("light"));
    await screen.findByRole("heading", { level: 1, name: "Nocturne is ready." });
    window.location.hash = "#/today";
    await waitFor(() => expect(document.documentElement.getAttribute("data-theme")).toBe("light"));
  });
});

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
    expect(
      screen.getByText(/Nothing leaves this computer unless you turn it on/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Get started" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Checking this computer…" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/computer memory and free space/).closest("[role=status]"),
    ).not.toBeNull();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("can be opened again from a finished setup", async () => {
    await open("done", "#/setup?step=welcome");
    await userEvent.click(await screen.findByRole("button", { name: "Get started" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: /lighter mind/ }),
    ).toBeInTheDocument();
  });

  it("offers Restore from a backup before Get started, in the setup card, at night", async () => {
    await open("notStarted", "#/setup", fakePlatform("light"));
    expect(
      await screen.findByRole("button", { name: "Restore from a backup" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restore from a backup" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Restore from a backup" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Moving from another computer? Choose the backup you made there, then type its passphrase.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back" })).toBeInTheDocument();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(screen.getByRole("main", { name: "Setting up Nocturne" })).toBeInTheDocument();
  });

  it("goes Back to the welcome card without changing anything", async () => {
    const { engine, platform } = await open("notStarted");
    const discard = vi.spyOn(engine, "discardRestore");
    const pick = vi.spyOn(platform, "pickBackup");
    await userEvent.click(await screen.findByRole("button", { name: "Restore from a backup" }));
    await userEvent.click(await screen.findByRole("button", { name: "Back" }));
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Keep anything. Nocturne sleeps on it and hands you one page each morning.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Get started" })).toBeInTheDocument();
    expect(discard).toHaveBeenCalled();
    expect(pick).not.toHaveBeenCalled();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("restores in setup, proceeds to checking, and says Done in the restored words", async () => {
    const platform = fakePlatform();
    const { engine } = await open("notStarted", "#/setup", platform);

    await restoreInSetup(engine, platform);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Checking this computer…" }),
    ).toBeInTheDocument();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

    const doneView = { ...(await engine.getSetup()), phase: "done" as const };
    vi.spyOn(engine, "getSetup").mockResolvedValue(doneView);
    await engine.startSetup("standard");

    expect(
      await screen.findByRole("heading", { level: 1, name: "Nocturne is ready." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Your memories from 12 Nov are all here. Your next page arrives tomorrow morning.",
      ),
    ).toBeInTheDocument();
  });
});

describe("what this computer can run", () => {
  it("shows the four tiers, the detected one chosen, and what it needs", async () => {
    await open("choosing", "#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Nocturne will use a lighter mind. Everything works, and tidying takes a little longer.",
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
      screen.getByText("Nocturne needs 8.4 GB of space. This computer has 220 GB free."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("It downloads once, and you can keep things straight away."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: nb("Set up · 8.4 GB") })).toBeInTheDocument();
  });

  it("says what each size is for and what it costs in words that read on their own", async () => {
    await open("choosing", "#/setup?step=choosing");
    const group = await screen.findByRole("group", { name: "How much to set up" });
    const card = (name: RegExp) => within(group).getByRole("radio", { name }).parentElement;
    expect(card(/^Large/)).toHaveTextContent("60 GB memory or more · 26 GB downloadFastest.");
    expect(card(/^Standard/)).toHaveTextContent("28 GB memory or more · 8.4 GB download");
    expect(card(/^Small/)).toHaveTextContent(
      "14 GB memory or more · 4.8 GB downloadSlower. Less memory, space and battery.",
    );
    expect(card(/^Tiny/)).toHaveTextContent(
      "6 GB memory or more · 3.2 GB downloadLeast memory, space and battery. Simpler answers.",
    );
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
    await userEvent.click(screen.getByRole("button", { name: nb("Set up · 3.2 GB") }));
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
    expect(screen.getByRole("button", { name: nb("Set up · 4.8 GB") })).toBeInTheDocument();
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
    expect(why.firstElementChild).toHaveAttribute("data-chevron");
    await userEvent.click(why);
    expect(why).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByText(
        "Nocturne picks the biggest mind this computer runs comfortably. It depends on computer memory.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("This computer has 32 GB.")).toBeInTheDocument();
    const sizes = screen.getByRole("table");
    expect(
      within(sizes)
        .getAllByRole("row")
        .map((row) => [...row.children].map((cell) => cell.textContent)),
    ).toEqual([
      ["Large", "60\u00a0GB or more"],
      ["Standard", "28\u00a0GB or more"],
      ["Small", "14\u00a0GB or more"],
      ["Tiny", "6\u00a0GB or more"],
    ]);
    expect(
      screen.getByText(
        "A smaller mind uses less memory, space and battery. Everything still works.",
      ),
    ).toBeInTheDocument();
  });

  it("starts setting up with the chosen tier and moves to getting ready", async () => {
    const { engine } = await open("choosing", "#/setup?step=choosing");
    const spy = vi.spyOn(engine, "startSetup");
    await userEvent.click(await screen.findByRole("button", { name: nb("Set up · 8.4 GB") }));
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
      screen.getByText("This computer has 5 GB of computer memory. Nocturne needs 6 GB or more."),
    ).toBeInTheDocument();
    expect(screen.getByText("Nothing has been downloaded.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Set up/ })).toBeNull();
  });

  it("says when there is not enough space, and offers to check again", async () => {
    await open("lowDisk", "#/setup?step=choosing");
    expect(
      await screen.findByRole("heading", { level: 1, name: /needs more free space/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("It has 3 GB free. Nocturne needs 5.2 GB to set up."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Free some space and check again. Nothing has been downloaded."),
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
      nb("Setting up Nocturne’s mind, 4.9 GB of 8.4 GB · about 17 min"),
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

  it("starts the engine again when it never started on a first run", async () => {
    const { engine } = await open("engineDown");
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn’t finish getting ready");
    const retry = vi.spyOn(engine, "retryEngine");
    const start = vi.spyOn(engine, "startSetup");
    const resume = vi.spyOn(engine, "resumeSetup");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(start).not.toHaveBeenCalled();
    expect(resume).not.toHaveBeenCalled();
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
    expect(await screen.findByText("Downloads you agreed to")).toBeInTheDocument();
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
