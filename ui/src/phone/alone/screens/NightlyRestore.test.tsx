import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { NativeRoot } from "../../native/NativeRoot";
import { nativeMockServices } from "../../testing/nativeMock";
import { SCRIPTED_KEY, SCRIPTED_PHRASE, scriptedBackup } from "../testing/backupMock";
import { scriptedNoteRows } from "../testing/homeMock";

afterEach(async () => {
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  scriptedNoteRows.clear();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  render(<NativeRoot services={await nativeMockServices()} />);
}

/** From Welcome, "I have a backup", then the backup online. */
async function fromWelcome(query = "") {
  await app(`m=first&s=welcome${query}`);
  fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
  expect(await screen.findByRole("heading", { name: "Where is your backup?" })).toBeVisible();
  expect(screen.getByRole("radio", { name: /Your backup in Google Drive/ })).toBeChecked();
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
}

/**
 * The phone comes back online. The waiting screen listens for it in an effect that can run after
 * its line shows, so it keeps saying so until the restore is asked for again.
 */
async function backOnline() {
  const restores = () => scriptedBackup.calls.filter((c) => c === "backup_restore").length;
  const before = restores();
  await waitFor(() => {
    window.dispatchEvent(new Event("online"));
    expect(restores()).toBeGreaterThan(before);
  });
}

function type(text: string) {
  fireEvent.change(screen.getByLabelText(/Your phrase|Recovery key/), { target: { value: text } });
  fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
}

describe("bringing back a backup onto an empty phone", () => {
  it("finds it, opens it with the phrase, brings it back and asks who makes the backups", async () => {
    await fromWelcome();
    expect(
      await screen.findByRole("heading", { name: "Bring back your memory" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Type the phrase for your backup.")).toBeInTheDocument();
    expect(screen.getByText(/Backup from today, 3:12 am · 1.4\sGB/)).toBeInTheDocument();
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeInTheDocument();
    expect(screen.getByText("2,140 things, every page and your journal.")).toBeInTheDocument();
    expect(screen.getByText(/Its mind stays off until you choose it again in/)).toHaveTextContent(
      "Its mind stays off until you choose it again in This phone.",
    );
    expect(
      screen.getByText(
        "Your calendar and Health on this phone stay off until you turn them on again.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", { name: "Make the backups from this phone?" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Back up from this phone" }));
    await waitFor(() => expect(scriptedBackup.calls).toContain("backup_make_in_use"));
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
  });

  it("says the health readings are read again from Health Connect, back to their first day", async () => {
    await fromWelcome("&health");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Health readings aren’t kept in your backup/)).toHaveTextContent(
      "Health readings aren’t kept in your backup. With Health on in This phone, Nocturne reads them again from Health Connect, back to 14 March 2025, once you allow older readings there.",
    );
  });

  it("says which health readings don't come back where Health Connect gives 30 days", async () => {
    await fromWelcome("&health=short");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    await screen.findByRole("heading", { name: "Your memory is back on this phone." });
    expect(
      screen.getByText(/lets Nocturne read only the last 30 days, so readings from before/),
    ).toHaveTextContent(/Pages and reflections that mention them stay\.$/);
  });

  it("says a wrong phrase, and after three tries points to the password manager", async () => {
    await fromWelcome();
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type("wrong words");
    expect(
      await screen.findByText(
        "That phrase doesn’t open this backup. Check each word, and try again.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Your phrase")).toHaveAttribute("aria-invalid", "true");
    const tries = () => scriptedBackup.calls.filter((c) => c === "backup_nights").length;
    for (const n of [2, 3]) {
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Bring it back" })).not.toHaveAttribute(
          "aria-disabled",
        ),
      );
      type("wrong words");
      await waitFor(() => expect(tries()).toBe(n));
    }
    expect(
      await screen.findByText(/If you saved it in a password manager, copy it from there/),
    ).toBeInTheDocument();
  });

  it("opens with the recovery key, and asks for a new phrase first", async () => {
    await fromWelcome();
    await screen.findByRole("heading", { name: "Bring back your memory" });
    fireEvent.click(screen.getByRole("button", { name: "Use the recovery key instead" }));
    expect(screen.getByText("Type the recovery key for your backup.")).toBeInTheDocument();
    type("K7QM-2XWD-9HRT-P4NC-B8LZ-6VFI");
    expect(
      await screen.findByText(
        "That recovery key doesn’t open this backup. Check each group of letters and numbers, and try again.",
      ),
    ).toBeInTheDocument();
    type(SCRIPTED_KEY.toLowerCase());
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Make a new phrase" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "You opened your backup with the recovery key. Make a new phrase now, so you have one.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.change(await screen.findByLabelText("Your phrase"), {
      target: { value: SCRIPTED_PHRASE },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(
      await screen.findByRole("heading", { name: "Make the backups from this phone?" }),
    ).toBeInTheDocument();
    expect(scriptedBackup.calls).toContain("backup_change_phrase");
  });

  it("picks an earlier night before the phrase, newest marked", async () => {
    await fromWelcome();
    await screen.findByRole("heading", { name: "Bring back your memory" });
    fireEvent.click(screen.getByRole("button", { name: "An earlier night…" }));
    expect(
      await screen.findByText("The newest is chosen unless you pick another."),
    ).toBeInTheDocument();
    const nights = screen.getAllByRole("radio");
    expect(nights).toHaveLength(7);
    expect(nights[0]).toBeChecked();
    expect(screen.getByText("Newest")).toBeInTheDocument();
    expect(screen.getAllByText(/1.4\sGB|1.3\sGB/)).toHaveLength(7);
    fireEvent.click(screen.getByRole("radio", { name: /Wednesday, 2:40 am/ }));
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(await screen.findByText(/Backup from Wednesday, 2:40 am/)).toBeInTheDocument();
    expect(scriptedBackup.calls.filter((c) => c === "backup_nights")).toHaveLength(1);
    type(SCRIPTED_PHRASE);
    await waitFor(() => expect(scriptedBackup.calls).toContain("backup_restore"));
  });

  it("checks the backup, then says the last step can't be stopped", async () => {
    await fromWelcome("&restore=slow&checking");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(await screen.findByText("Checking the backup")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
  });

  it("offers no Stop for its last step", async () => {
    await fromWelcome("&restore=slow&adding");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByText("Almost done. This last step can’t be stopped."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
  });

  it("asks before coming down over mobile data, and can wait for Wi-Fi", async () => {
    await fromWelcome("&net=mobile");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByText(
        /This is about 1.4\sGB. On mobile data, it may count against your plan./,
      ),
    ).toBeInTheDocument();
    expect(scriptedBackup.calls).not.toContain("backup_restore");
    fireEvent.click(screen.getByRole("button", { name: "Wait for Wi-Fi" }));
    expect(
      await screen.findByText("Waiting for Wi-Fi. It starts when this phone is on Wi-Fi."),
    ).toBeInTheDocument();
    scriptedBackup.wifi = true;
    await backOnline();
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeInTheDocument();
  });

  it("goes on over mobile data when asked to", async () => {
    await fromWelcome("&net=mobile");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    fireEvent.click(await screen.findByRole("button", { name: "Go on" }));
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeInTheDocument();
  });

  it("back online, carries on over mobile data once the person said so", async () => {
    await fromWelcome("&restore=offline&net=mobile");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    fireEvent.click(await screen.findByRole("button", { name: "Go on" }));
    expect(await screen.findByText(/Waiting for the internet/)).toBeInTheDocument();
    await backOnline();
    expect(scriptedBackup.calls.filter((c) => c === "backup_restore")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Go on" })).toBeNull();
  });

  it("names both versions when a newer Nocturne made it", async () => {
    await fromWelcome("&restore=newer");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByRole("heading", { name: "This backup is from a newer Nocturne" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "It was made with Nocturne 0.9.0, and this phone has 0.8.0. Update Nocturne from Google Play, then try again. Nothing was changed.",
      ),
    ).toBeInTheDocument();
  });

  it("waits for the internet and carries on", async () => {
    await fromWelcome("&restore=offline");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByText(
        "Waiting for the internet. This carries on when this phone is back online.",
      ),
    ).toBeInTheDocument();
    scriptedBackup.wifi = true;
    await backOnline();
    expect(
      await screen.findByRole("heading", { name: "Your memory is back on this phone." }),
    ).toBeInTheDocument();
  });

  it("says when part of the newest night is missing, and offers an earlier one", async () => {
    await fromWelcome("&restore=missing");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByRole("heading", { name: "Part of this backup is missing" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Choose an earlier night" }));
    expect(screen.getByRole("radio", { name: /Thursday, 2:58 am/ })).toBeChecked();
  });

  it("says when there is no backup in the account, or no internet", async () => {
    await fromWelcome("&found=none");
    expect(
      await screen.findByRole("heading", {
        name: "No backup found in this Google account’s Drive.",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try another account" })).toBeInTheDocument();
    cleanup();
    clearMocks();
    await fromWelcome("&found=offline");
    expect(
      await screen.findByRole("heading", {
        name: "This phone isn’t online. Finding your backup needs the internet.",
      }),
    ).toBeInTheDocument();
  });

  it("on an iPhone, says to sign in with the old phone's Apple Account", async () => {
    await app("m=first&s=welcome&os=ios&found=none");
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    expect(await screen.findByRole("radio", { name: /Your backup in iCloud/ })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByText("Sign in with the Apple Account your old phone used."),
    ).toBeInTheDocument();
  });

  it("says when Google didn't allow it, with Try again", async () => {
    await fromWelcome("&connect=refused");
    expect(
      await screen.findByText("Google didn’t allow it, so Nocturne can’t look for your backup."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("can be stopped while it comes down, and nothing changed", async () => {
    await fromWelcome("&restore=slow");
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(await screen.findByText("Getting your backup")).toBeInTheDocument();
    expect(await screen.findByText(/650\sMB of 1.4\sGB/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(await screen.findByRole("heading", { name: "Stopped." })).toBeInTheDocument();
    expect(screen.getByText("Nothing on this phone changed.")).toBeInTheDocument();
  });
});

describe("bringing a backup into a phone that has a memory", () => {
  async function toMerge(query = "") {
    await app(`m=kept&v=thisPhone${query}`);
    fireEvent.click(await screen.findByRole("button", { name: /Restore a backup…/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    await screen.findByRole("heading", { name: "Bring back your memory" });
    type(SCRIPTED_PHRASE);
    expect(
      await screen.findByRole("heading", {
        name: "Your backup has its own memory, and this phone has one too.",
      }),
    ).toBeInTheDocument();
  }

  it("says both sides, then brings its things in", async () => {
    await toMerge();
    const inBackup = screen.getByText("In the backup").parentElement as HTMLElement;
    expect(inBackup).toHaveTextContent(
      /2,120 things, from Sunday 2 March to Friday 13 November · about 1.4\sGB/,
    );
    const here = screen.getByText("On this phone").parentElement as HTMLElement;
    expect(here).toHaveTextContent("412 things, from Saturday 20 June to today");
    expect(
      screen.getByText(
        "A phone holds one person’s memory, so the backup’s things join the one here.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Bring them in" }));
    expect(
      await screen.findByRole("heading", { name: "Your backup’s things are here." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "1,900 things, 240 journal entries and 118 check-ins were added to the memory on this phone.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("12 were already here, so they were kept once.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", { name: "Make the backups from this phone?" }),
    ).toBeInTheDocument();
  });

  it("asks before stopping, and stops only until adding begins", async () => {
    await toMerge("&restore=slow");
    fireEvent.click(screen.getByRole("button", { name: "Bring them in" }));
    expect(await screen.findByText("Getting your things ready…")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    const ask = await screen.findByRole("dialog", {
      name: "Stop bringing your backup in? Nothing changes on this phone.",
    });
    fireEvent.click(within(ask).getByRole("button", { name: "Keep going" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Stopped. Nothing changed on this phone." }),
    ).toBeInTheDocument();
  });

  it("offers no Stop once its things are being added", async () => {
    await toMerge("&restore=slow&adding");
    fireEvent.click(screen.getByRole("button", { name: "Bring them in" }));
    expect(await screen.findByText("Adding them to the memory on this phone…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    expect(screen.getByText("Keep Nocturne open until this finishes.")).toBeInTheDocument();
  });

  it("does not ask who makes the backups when it was this phone's own", async () => {
    await toMerge("&own");
    fireEvent.click(screen.getByRole("button", { name: "Bring them in" }));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("button", { name: /Restore a backup…/ })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Make the backups from this phone?" })).toBeNull();
  });

  it("Not now leaves the phone as it was", async () => {
    await toMerge();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(await screen.findByRole("button", { name: /Restore a backup…/ })).toBeInTheDocument();
    expect(scriptedBackup.calls).not.toContain("backup_restore");
  });
});
