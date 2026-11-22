import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { HomeStats } from "../../engine/bindings";
import { PhoneRequestError } from "../engine";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { createHomeApi, HomeRequestError } from "./api";
import { createLocalEngine } from "./engine";
import { memoryNotes, REMINDER_DAYS, reminderDone, reminderDue } from "./reminder";
import { scriptedNoteRows } from "./testing/homeMock";

afterEach(async () => {
  // A step back the screens asked for lands before the next test starts.
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  scriptedNoteRows.clear();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

const nav = () => screen.getByRole("navigation", { name: "Nocturne" });

describe("Welcome and starting on this phone", () => {
  it("offers two ways in and a backup, then starts with the phone alone", async () => {
    await app("m=first&s=welcome");
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect to my computer" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "I have a backup" })).toBeInTheDocument();
    expect(screen.getByText("No account and no sign-in.")).toBeInTheDocument();
    expect(screen.queryByText(/household/i)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Start on this phone" }));
    expect(
      await screen.findByRole("heading", { name: "Your memory lives on this phone" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Find anything you kept by its words")).toBeInTheDocument();
    expect(screen.getByText("Reflections and recaps")).toBeInTheDocument();
    expect(
      screen.getByText(
        "A mind can come from Nocturne on your own computer. Your memory then moves there.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText(/Save a backup now and then in/)).toHaveTextContent(
      "If this phone is lost, what you kept goes with it. Save a backup now and then in This phone.",
    );

    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(
      await screen.findByText(/Nocturne can show what’s in this phone’s calendar/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "Your page needs a mind to write it." }),
    ).toBeInTheDocument();
  });

  it("starts with the app's own name when the phone gives none", async () => {
    history.replaceState(null, "", "/?m=first&s=welcome");
    const services = await nativeMockServices();
    services.deviceName = async () => null;
    render(<NativeRoot services={services} />);
    fireEvent.click(await screen.findByRole("button", { name: "Start on this phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    expect(
      await screen.findByText(/Nocturne can show what’s in this phone’s calendar/),
    ).toBeInTheDocument();
  });

  it("ignores a companion's screen left in the history", async () => {
    history.replaceState(
      { view: { name: "relinkWords", busy: false, error: false } },
      "",
      "/?m=fresh",
    );
    render(<NativeRoot services={await nativeMockServices()} />);
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
  });

  it("opens the pairing from Connect to my computer", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "Connect to my computer" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne lives on your computer" }),
    ).toBeInTheDocument();
  });
});

describe("Today on a phone alone", () => {
  it("shows the no-mind card, the check-in and the first day's empty line", async () => {
    await app("m=fresh");
    expect(await screen.findByText("Saturday 21 November")).toBeInTheDocument();
    const card = screen.getByRole("region", { name: "Your page needs a mind to write it." });
    expect(within(card).getByRole("button", { name: "Choose a mind" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^How are you feeling\?\s*Check in$/ }),
    ).toBeVisible();
    expect(screen.getByText(/Nothing kept yet today\./)).toHaveTextContent(
      "Nothing kept yet today. Tap Keep to keep something.",
    );
    expect(screen.getByText("Nothing in your calendar today.")).toBeInTheDocument();
    expect(screen.queryByText(/No backup yet/)).toBeNull();
  });

  it("lists what was kept today, with what waits to be read", async () => {
    await app("m=kept");
    expect(await screen.findByRole("heading", { name: "Kept today · 2" })).toBeInTheDocument();
    expect(screen.getByText("Ask about the swap for the 28th")).toBeInTheDocument();
    expect(screen.getByText("waiting to be read")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "See today’s journal" }));
    expect(within(nav()).getByRole("button", { name: "Journal" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(await screen.findByText("Reflections need a mind.")).toBeInTheDocument();
    expect(screen.getByText("waiting to be read")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Reflect on/ })).toBeNull();
  });

  it("shows the lost-phone reminder once", async () => {
    await app("m=reminder");
    expect(
      await screen.findByText("No backup yet. If this phone is lost, what you kept goes with it."),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByText(/No backup yet\./)).toBeNull();
    cleanup();
    clearMocks();
    history.replaceState(null, "", "/?m=kept");
    render(<NativeRoot services={await nativeMockServices()} />);
    expect(await screen.findByRole("heading", { name: "Kept today · 2" })).toBeInTheDocument();
    expect(screen.queryByText(/No backup yet\./)).toBeNull();
  });

  it("opens Give Nocturne a mind from the card", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
    expect(await screen.findByRole("heading", { name: "Give Nocturne a mind" })).toBeVisible();
    expect(screen.getByText(/there, and scan its code from this phone\./)).toHaveTextContent(
      "Choose Add a phone there, and scan its code from this phone.",
    );
    expect(screen.getByRole("button", { name: "Scan the code" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
  });
});

describe("Ask finds by words", () => {
  it("says what it does before typing, and when nothing is kept", async () => {
    await app("m=fresh");
    await screen.findByText("Your memory is on this phone");
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(
      await screen.findByText("Nothing kept yet. What you keep can be found here by its words."),
    ).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Find something you kept")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Keep" })).toBeNull();
  });

  it("marks the matched words, and says when nothing has them", async () => {
    await app("m=kept");
    await screen.findByText("Your memory is on this phone");
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText("Find anything you kept by its words.")).toBeInTheDocument();
    const field = screen.getByRole("textbox", { name: "Find something you kept" });
    fireEvent.change(field, { target: { value: "swap" } });
    expect(await screen.findByText("Found by their words. Answers need a mind.")).toBeVisible();
    const first = await screen.findByRole("button", {
      name: /^Ask about the swap for the 28th, kept .+, Note$/,
    });
    expect(within(first).getByText("swap").tagName).toBe("MARK");
    expect(screen.getByRole("button", { name: /^Jo can swap earlies/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose a mind" })).toBeInTheDocument();

    fireEvent.change(field, { target: { value: "lighthouse" } });
    expect(await screen.findByText("Nothing you kept has those words.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(await screen.findByText("Find anything you kept by its words.")).toBeInTheDocument();
  });
});

describe("This phone, a backup and Delete everything", () => {
  it("shows where the memory is, its mind and the actions", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    const memory = await screen.findByRole("region", { name: "Your memory" });
    expect(await within(memory).findByText("On this phone · 412 MB")).toBeInTheDocument();
    expect(within(memory).getByRole("button", { name: /Its mind.*None yet/ })).toBeVisible();
    expect(screen.getByText(/Removing the app removes your memory/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Save a backup….*No backup yet/ })).toBeVisible();
    expect(screen.getByRole("button", { name: "Move my memory…" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Delete everything…" })).toBeVisible();
    expect(screen.getByText("Nocturne 0.1.0", { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Privacy policy" })).toBeVisible();
    expect(screen.queryByText("Name")).toBeNull();
    expect(screen.queryByText(/locked copy/i)).toBeNull();
    const voice = await screen.findByRole("button", { name: /Voice notes in words.*Off.*Turn on/ });
    fireEvent.click(voice);
    await waitFor(() => expect(screen.queryByText("Voice notes in words")).toBeNull());
  });

  it("saves a backup with a passphrase, to the place the phone's sheet gives", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: /Save a backup…/ }));
    expect(await screen.findByText(/One file with everything you kept/)).toBeInTheDocument();
    const save = screen.getByRole("button", { name: "Save the backup…" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText("Next, choose where to save it.")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Choose a passphrase"), {
      target: { value: "copper kettle orbit" },
    });
    fireEvent.change(screen.getByLabelText("Type it again"), {
      target: { value: "copper kettle orbits" },
    });
    fireEvent.click(screen.getByLabelText("I’ve written it down or saved it somewhere safe"));
    fireEvent.click(save);
    expect(await screen.findByText("The two passphrases don’t match.")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Type it again"), {
      target: { value: "copper kettle orbit" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save the backup…" }));
    expect(await screen.findByRole("heading", { name: "Your backup is saved." })).toBeVisible();
    expect(screen.getByText(/^412 MB, made /)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(
      await screen.findByRole("button", { name: /Save a backup….*Last backup/ }, { timeout: 4000 }),
    ).toBeVisible();
  }, 15_000);

  it("deletes everything only once the words are typed", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
    expect(
      await screen.findByRole("heading", { name: "Delete everything on this phone?" }),
    ).toBeVisible();
    const yes = screen.getByRole("button", { name: "Delete everything" });
    expect(yes).toHaveAttribute("aria-disabled", "true");
    fireEvent.change(screen.getByLabelText("Type “delete everything” to go ahead"), {
      target: { value: "delete everything" },
    });
    expect(yes).not.toHaveAttribute("aria-disabled");
    fireEvent.click(yes);
    expect(await screen.findByText(/Nothing kept yet today\./)).toBeInTheDocument();
  });
});

describe("a memory that can't be opened, and bringing back a backup", () => {
  it("puts the damaged memory aside and brings a backup back", async () => {
    await app("m=damaged");
    expect(
      await screen.findByRole("heading", {
        name: "Nocturne couldn’t open your memory on this phone.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Get help" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "I have a backup" }));
    expect(await screen.findByRole("heading", { name: "Bring back your memory" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Choose the file" }));
    expect(await screen.findByText("nocturne-backup-2026-11-21.nocturne")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Passphrase for this backup"), {
      target: { value: "not the one" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(
      await screen.findByText(
        "That passphrase doesn’t open this backup. Check capital letters, and try again.",
      ),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Passphrase for this backup"), {
      target: { value: "correct horse battery" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeVisible();
    expect(screen.getByText("128 things, every page and your journal.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Kept today · 2" })).toBeInTheDocument();
  });
});

describe("a backup over a memory that is already here", () => {
  it("never replaces what is here without asking", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    fireEvent.change(await screen.findByLabelText("Passphrase for this backup"), {
      target: { value: "already here" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(
      await screen.findByRole("heading", { name: "Replace what’s on this phone?" }),
    ).toBeVisible();
    expect(screen.getByText(/You can bring back what was here from/)).toHaveTextContent(
      "Restoring puts what’s on this phone now aside, then brings in the backup. Nothing is deleted. You can bring back what was here from This phone.",
    );
    expect(screen.getByText("On this phone now · Put aside, not deleted")).toBeInTheDocument();
    expect(screen.getByText("Kept after the backup")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Replace with the backup" }));
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeVisible();
  });

  it("keeps what is here when the person says so", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    expect(await screen.findByText(/^412 MB · made /)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Passphrase for this backup"), {
      target: { value: "already here" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    fireEvent.click(await screen.findByRole("button", { name: "Keep what’s here" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeVisible();
  });

  it("says a backup is from a newer Nocturne, or damaged, in 0013's words", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    const field = await screen.findByLabelText("Passphrase for this backup");
    fireEvent.change(field, { target: { value: "from the future" } });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(await screen.findByText("This backup is from a newer Nocturne")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Passphrase for this backup"), {
      target: { value: "torn pages" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(await screen.findByText("Part of this backup is damaged")).toBeInTheDocument();
  });
});

describe("keeping and an item on a phone alone", () => {
  it("says a kept note is on today's journal, and nothing of a computer", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Keep" }));
    expect(screen.queryByText(/On your computer/)).toBeNull();
    fireEvent.change(await screen.findByLabelText(/Type something to keep/), {
      target: { value: "Bins go out on Tuesday" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(
      await screen.findByText("It’s on today’s journal, and Ask can find it by its words."),
    ).toBeInTheDocument();
    expect(screen.getByText("Changed your mind?")).toBeInTheDocument();
    expect(screen.queryByText(/computer/)).toBeNull();
  });

  it("says when there was no room to keep it", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Keep" }));
    fireEvent.change(await screen.findByLabelText(/Type something to keep/), {
      target: { value: "no room left" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(
      await screen.findByText("There wasn’t room to keep this. Free some room and try again."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("says why before Android first asks for the microphone", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Keep" }));
    fireEvent.click(await screen.findByRole("button", { name: /Record/ }));
    expect(await screen.findByText(/^Next, Android asks to let Nocturne use/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(screen.queryByText(/^Next, Android asks/)).toBeNull());
  });

  it("marks a kept thing private on its page", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: /Ask about the swap/ }));
    const toggle = await screen.findByRole("switch", { name: /Private/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByText("Kept on this phone and never sent to a mind online or at home."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/your computer/)).toBeNull();
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
  }, 15_000);
});

describe("the phone's own memory behind the screens", () => {
  it("answers the shared calls from home_call and says answers need a mind", async () => {
    mockIPC((cmd, args) => {
      if (cmd !== "home_call") return null;
      const call = (args as { call: { cmd: string } }).call;
      return call.cmd === "getStatus"
        ? { type: "status", status: { readiness: "ready", pageDate: null, needsYou: 0 } }
        : { type: "error", code: "notFound" };
    });
    const engine = createLocalEngine({ deviceName: () => "Maya’s phone", version: "1.0.0" });
    expect(engine.connection().kind).toBe("connected");
    expect((await engine.status()).readiness).toBe("ready");
    await expect(engine.getItem("gone")).rejects.toEqual(new PhoneRequestError("notFound"));
    await expect(engine.ask("When is the dentist?", () => undefined)).rejects.toEqual(
      new PhoneRequestError("notReady"),
    );
  });

  it("turns a home error into a code the screens read", async () => {
    mockIPC((cmd) => (cmd === "home_local" ? { type: "error", code: "wrongPassphrase" } : null), {
      shouldMockEvents: true,
    });
    const home = createHomeApi();
    await expect(home.checkBackup("/b", "x", () => undefined)).rejects.toEqual(
      new HomeRequestError("wrongPassphrase"),
    );
  });
});

describe("the lost-phone reminder", () => {
  const stats = (items: number, backedUp = false): HomeStats => ({
    counts: { items, memories: 0, pages: 0, originals: 0 },
    bytes: 0,
    lastKept: null,
    lastBackup: backedUp ? { at: "2026-11-01T09:00:00Z", bytes: 1 } : null,
  });
  const DAY = 86_400_000;

  it("comes after 30 days of keeping with no backup, and only once", async () => {
    const notes = memoryNotes();
    expect(await reminderDue(stats(0), 0, notes)).toBe(false);
    expect(await reminderDue(stats(1), 1000, notes)).toBe(false);
    expect(await reminderDue(stats(3), 1000 + (REMINDER_DAYS - 1) * DAY, notes)).toBe(false);
    expect(await reminderDue(stats(3), 1000 + REMINDER_DAYS * DAY, notes)).toBe(true);
    expect(await reminderDue(stats(3, true), 1000 + REMINDER_DAYS * DAY, notes)).toBe(false);
    await reminderDone(notes);
    expect(await reminderDue(stats(3), 1000 + 90 * DAY, notes)).toBe(false);
  });
});

it("keeps the companion app as it was when the phone is paired", async () => {
  await app("");
  await waitFor(() => expect(screen.getByText("Connected to Home computer")).toBeInTheDocument());
});
