import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { setHourClock } from "../../copy/format";
import type { HomeStats } from "../../engine/bindings";
import { PhoneRequestError } from "../engine";
import { CLOCK_CHANGED, phoneIs24Hour } from "../native/clock";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { createHomeApi, HomeRequestError } from "./api";
import { createLocalEngine } from "./engine";
import { memoryNotes, REMINDER_DAYS, reminderDone, reminderDue } from "./reminder";
import { scriptedNoteRows } from "./testing/homeMock";

/** "Where is your backup?": a backup file. */
async function toFile() {
  fireEvent.click(await screen.findByRole("radio", { name: /A backup file/ }));
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
}

afterEach(async () => {
  // A step back the screens asked for lands before the next test starts.
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  scriptedNoteRows.clear();
  setHourClock(null);
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
    expect(screen.getByText(/Turn on backup in/)).toHaveTextContent(
      "If this phone is lost, what you kept goes with it. Turn on backup in This phone.",
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

  it("says after midnight on the date line until the journal day starts at 4 am", async () => {
    history.replaceState(null, "", "/?m=fresh");
    const services = await nativeMockServices();
    services.now = () => new Date(2026, 10, 21, 0, 30).getTime();
    render(<NativeRoot services={services} />);
    expect(await screen.findByText("Saturday 21 November, after midnight")).toBeInTheDocument();
  });

  it("has no after midnight once the journal day has started", async () => {
    history.replaceState(null, "", "/?m=fresh");
    const services = await nativeMockServices();
    services.now = () => new Date(2026, 10, 21, 4, 0).getTime();
    render(<NativeRoot services={services} />);
    expect(await screen.findByText("Saturday 21 November")).toBeInTheDocument();
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

  it("says an empty journal day holds what you keep, with no computer in it", async () => {
    await app("m=fresh");
    await screen.findByText("Your memory is on this phone");
    fireEvent.click(within(nav()).getByRole("button", { name: "Journal" }));
    expect(await screen.findByText("Anything you keep shows up here.")).toBeInTheDocument();
    expect(screen.queryByText(/computer/)).toBeNull();
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

  it("opens Give Nocturne a mind from the card, by a computer of one's own", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
    expect(
      await screen.findByRole("heading", { name: "Where should Nocturne’s mind be?" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("radio", { name: /^A computer of your own/ }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Give Nocturne a mind" })).toBeVisible();
    expect(screen.getByText(/there, and scan its code from this phone\./)).toHaveTextContent(
      "Choose Add a phone there, and scan its code from this phone.",
    );
    expect(screen.getByRole("button", { name: "Scan the code" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(
      await screen.findByRole("heading", { name: "Where should Nocturne’s mind be?" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
  });
});

describe("Back on Android", () => {
  const current = () => within(nav()).getByRole("button", { current: "page" }).textContent;

  it("goes from Journal or Ask to Today, and Today is where Back leaves", async () => {
    await app("m=fresh");
    await screen.findByText("Your memory is on this phone");
    fireEvent.click(within(nav()).getByRole("button", { name: "Journal" }));
    await waitFor(() => expect(current()).toBe("Journal"));
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    await waitFor(() => expect(current()).toBe("Ask"));
    const length = history.length;
    history.back();
    await waitFor(() => expect(current()).toBe("Today"));
    expect(history.length).toBe(length);
    expect(history.state.view).toEqual({ name: "tab", tab: "today" });
  });

  it("does not stack the tabs when Today is chosen again", async () => {
    await app("m=fresh");
    await screen.findByText("Your memory is on this phone");
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    await waitFor(() => expect(current()).toBe("Ask"));
    fireEvent.click(within(nav()).getByRole("button", { name: "Today" }));
    await waitFor(() => expect(current()).toBe("Today"));
    expect(history.state.view).toEqual({ name: "tab", tab: "today" });
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
    scriptedNoteRows.set("voiceAsked", "1");
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
    await app("m=kept&s=welcome&g=calendar,health");
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
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Everything was deleted from this phone.");
    expect(
      screen.getByText(
        "Your phone still lets Nocturne read your calendar and Health until you take that back.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "How to take that back" }));
    expect(await screen.findByRole("heading", { name: "Your phone’s permissions" })).toBeVisible();
    expect(
      screen.getByText(
        /^Nocturne reads your calendar and Health, and uses the microphone, only while your phone allows it\./,
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open phone settings" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Everything was deleted from this phone.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Start on this phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    expect(
      await screen.findByText(/Nocturne can show what’s in this phone’s calendar/),
    ).toBeInTheDocument();
  });
});

describe("Welcome after Delete everything says only what is still allowed", () => {
  const cases: [string, string | null][] = [
    ["calendar", "read your calendar"],
    ["health", "read Health"],
    ["microphone", "use the microphone"],
    ["calendar,health", "read your calendar and Health"],
    ["calendar,microphone", "read your calendar and use the microphone"],
    ["health,microphone", "read Health and use the microphone"],
    ["calendar,health,microphone", "read your calendar and Health, and use the microphone"],
    ["", null],
  ];

  for (const [granted, what] of cases) {
    it(what ? `names ${what}` : "has no line and no link with nothing allowed", async () => {
      await app(`m=kept&s=welcome&g=${granted}`);
      fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
      fireEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
      fireEvent.change(await screen.findByLabelText("Type “delete everything” to go ahead"), {
        target: { value: "delete everything" },
      });
      fireEvent.click(screen.getByRole("button", { name: "Delete everything" }));
      expect(await screen.findByRole("status")).toHaveTextContent(
        "Everything was deleted from this phone.",
      );
      const line = `Your phone still lets Nocturne ${what} until you take that back.`;
      if (what) {
        expect(screen.getByText(line)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "How to take that back" })).toBeVisible();
      } else {
        expect(screen.queryByText(/Your phone still lets/)).toBeNull();
        expect(screen.queryByRole("button", { name: "How to take that back" })).toBeNull();
      }
    });
  }
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
    expect(await screen.findByRole("heading", { name: "Where is your backup?" })).toBeVisible();
    await toFile();
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
  it("brings its things in after asking, and puts nothing aside", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    await toFile();
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    fireEvent.change(await screen.findByLabelText("Passphrase for this backup"), {
      target: { value: "already here" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(
      await screen.findByRole("heading", {
        name: "Your backup has its own memory, and this phone has one too.",
      }),
    ).toBeVisible();
    const inBackup = screen.getByText("In the backup").parentElement as HTMLElement;
    expect(inBackup).toHaveTextContent(/128 things, up to .* · about/);
    const here = screen.getByText("On this phone").parentElement as HTMLElement;
    expect(here).toHaveTextContent("2 things");
    expect(screen.queryByText(/Put aside, not deleted/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Bring them in" }));
    expect(
      await screen.findByRole("heading", { name: "Your backup’s things are here." }),
    ).toBeVisible();
    expect(
      screen.getByText(
        "126 things, 52 journal entries and 6 check-ins were added to the memory on this phone.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("2 were already here, so they were kept once.")).toBeInTheDocument();
  });

  it("is reached from This phone, and always asks before bringing things in", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    expect(
      await screen.findByText(
        "A backup’s things join what’s on this phone. Nothing here is deleted.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Restore a backup/ }));
    await toFile();
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    fireEvent.change(await screen.findByLabelText("Passphrase for this backup"), {
      target: { value: "correct horse battery" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    expect(
      await screen.findByRole("heading", {
        name: "Your backup has its own memory, and this phone has one too.",
      }),
    ).toBeVisible();
    await waitFor(() =>
      expect(screen.getByText("On this phone").parentElement).toHaveTextContent(
        "412 things, from Saturday 20 June to today",
      ),
    );
  }, 15_000);

  it("leaves what is here as it was with Not now", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    await toFile();
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    expect(await screen.findByText("412 MB")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Passphrase for this backup"), {
      target: { value: "already here" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeVisible();
  });

  it("says a backup is from a newer Nocturne, or damaged, in 0013's words", async () => {
    await app("m=first&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    await toFile();
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

  it("shows the time as the phone's clock is set", async () => {
    await app("m=fresh&h=24");
    setHourClock(await phoneIs24Hour());
    fireEvent.click(await screen.findByRole("button", { name: "Keep" }));
    fireEvent.change(await screen.findByLabelText(/Type something to keep/), {
      target: { value: "Bins go out on Tuesday" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByText(/^Note · \d{2}:\d{2}$/)).toBeInTheDocument();
  }, 15_000);

  it("draws the times again when the phone's clock setting changes while away", async () => {
    await app("m=kept");
    setHourClock(false);
    await screen.findByRole("heading", { name: "Kept today · 2" });
    expect(screen.getAllByText(/^\d{1,2}:\d{2} (am|pm)$/).length).toBeGreaterThan(0);
    setHourClock(true);
    act(() => {
      window.dispatchEvent(new Event(CLOCK_CHANGED));
    });
    await waitFor(() => expect(screen.getAllByText(/^\d{2}:\d{2}$/).length).toBeGreaterThan(0));
    expect(screen.queryAllByText(/^\d{1,2}:\d{2} (am|pm)$/)).toHaveLength(0);
  }, 15_000);

  it("says a saved check-in is on the phone, with no name and no time to change its mind", async () => {
    await app("m=fresh");
    fireEvent.click(
      await screen.findByRole("button", { name: /^How are you feeling\?\s*Check in$/ }),
    );
    fireEvent.click(await screen.findByRole("radio", { name: "Settled" }));
    const sheet = await screen.findByRole("dialog", { name: "Words for Settled" });
    fireEvent.click(within(sheet).getByRole("button", { name: "relieved" }));
    fireEvent.click(within(sheet).getByRole("button", { name: "Save" }));
    await screen.findByText(/^Check-in · /);
    expect(screen.getByText(/^Check-in · \d{1,2}:\d{2}( am| pm)?$/)).toBeInTheDocument();
    expect(screen.getByText("Changed your mind?")).toBeInTheDocument();
    expect(screen.queryByText(/kept from|Until|too late|computer/)).toBeNull();
  }, 15_000);

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

  it("opens no microphone when the person says Not now", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Keep" }));
    fireEvent.click(await screen.findByRole("button", { name: /Record/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByText(/^Next, Android asks/)).toBeNull());
    expect(screen.getByLabelText(/Type something to keep/)).toBeInTheDocument();
  });

  it("marks a kept thing private on its page", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: /Ask about the swap/ }));
    const toggle = await screen.findByRole("switch", { name: /Private/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByText(
        "Private things don’t come up on their own in notifications, reminders or updates. They still help answer what you ask. If your memory moves, it moves too, locked like the rest.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/your computer/)).toBeNull();
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
  }, 15_000);
});

describe("the phone's own memory behind the screens", () => {
  it("answers the shared calls from home_call, asks the mind by name and stops it", async () => {
    const stopped: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "home_cancel_ask") return void stopped.push(args);
      if (cmd === "home_ask")
        return (args as { question: string }).question === "When is the dentist?"
          ? { type: "error", code: "notReady" }
          : {
              type: "askDelta",
              delta: {
                askId: "a1",
                phase: "done",
                textDelta: "On Thursday at 9.",
                receipts: [],
                memoryIds: [],
                near: [],
                found: 1,
                etaSeconds: null,
                plain: true,
              },
            };
      if (cmd !== "home_call") return null;
      const call = (args as { call: { cmd: string } }).call;
      if (call.cmd === "getStatus")
        return { type: "status", status: { readiness: "ready", pageDate: null, needsYou: 0 } };
      return { type: "error", code: "notFound" };
    });
    const engine = createLocalEngine({ deviceName: () => "Maya’s phone", version: "1.0.0" });
    expect(engine.connection().kind).toBe("connected");
    expect((await engine.status()).readiness).toBe("ready");
    await expect(engine.getItem("gone")).rejects.toEqual(new PhoneRequestError("notFound"));
    await expect(engine.ask("When is the dentist?", () => undefined)).rejects.toEqual(
      new PhoneRequestError("notReady"),
    );
    const heard: string[] = [];
    await engine.ask("What time is it?", (d) => heard.push(`${d.phase}:${d.textDelta}`));
    expect(heard).toEqual(["searching:null", "done:On Thursday at 9."]);
    const id = await engine.ask("Again?", () => undefined);
    await engine.cancelAsk(id);
    expect(stopped).toEqual([{ askId: id }]);
  });

  it("tells where the question is looking while the mind looks further", async () => {
    const step = (askId: string, phase: string) => ({
      askId,
      phase,
      textDelta: null,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
      plain: false,
    });
    mockIPC(
      async (cmd, args) => {
        if (cmd !== "home_ask") return null;
        const askId = (args as { askId: string }).askId;
        await emit("home-ask-step", step("another", "lookingJournal"));
        await emit("home-ask-step", step(askId, "lookingCalendar"));
        return { type: "askDelta", delta: { ...step("a1", "done"), textDelta: "Tomorrow." } };
      },
      { shouldMockEvents: true },
    );
    const engine = createLocalEngine({ deviceName: () => "Maya’s phone", version: "1.0.0" });
    const heard: string[] = [];
    await engine.ask("What’s on tomorrow?", (d) => heard.push(d.phase));
    expect(heard).toEqual(["searching", "lookingCalendar", "done"]);
    // Nothing is heard of a question once it has ended.
    await emit("home-ask-step", step("a1", "lookingSearch"));
    expect(heard).toHaveLength(3);
  });

  it("keeps a shared voice note under its own name, and a recorded one under none", async () => {
    const begun: { cmd: string; caption?: string | null }[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "keep_begin") {
        begun.push((args as { call: { cmd: string; caption?: string | null } }).call);
        return 1;
      }
      if (cmd === "home_keep_end")
        return {
          type: "kept",
          kept: {
            item: {
              id: "x",
              kind: "audio",
              title: "t",
              keptAt: "2026-11-21T07:05:00.000Z",
              durationMs: null,
              pages: null,
              hasOriginal: true,
              summary: null,
              text: null,
              truncated: false,
            },
            duplicate: false,
            takeBackUntil: "2026-11-21T07:10:00.000Z",
          },
        };
      return null;
    });
    const engine = createLocalEngine({ deviceName: () => "Maya’s phone", version: "1.0.0" });
    const sound = new Blob([Uint8Array.from(new TextEncoder().encode("RIFF\0\0\0\0WAVEfmt "))], {
      type: "audio/wav",
    });
    await engine.keepRecording(sound, "c-1", { name: "Interview 3" });
    await engine.keepRecording(sound, "c-2");
    expect(begun[0]).toMatchObject({ cmd: "keepBytes", caption: "Interview 3" });
    expect(begun[1]?.cmd).toBe("keepRecording");
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
