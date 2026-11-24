import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { NativeRoot } from "../../native/NativeRoot";
import { nativeMockServices } from "../../testing/nativeMock";
import { NEW_KEY, SCRIPTED_PHRASE, scriptedBackup } from "../testing/backupMock";
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

const nightly = () => screen.findByRole("switch", { name: /Back up every night/ });

describe("Backup in This phone", () => {
  it("is a row under Your memory that says Off, or when the last backup was", async () => {
    await app("m=kept&v=thisPhone");
    expect(await screen.findByRole("button", { name: /^Backup\s*Off/ })).toBeInTheDocument();
    cleanup();
    clearMocks();
    await app("m=kept&v=thisPhone&b=on");
    const row = await screen.findByRole("button", {
      name: /^Backup\s*Last backup today, 3:12 am · 1.4\sGB in your Google Drive/,
    });
    fireEvent.click(row);
    expect(await nightly()).toHaveAttribute("aria-checked", "true");
  });

  it("shows the rows and what to know about the backup while it is on", async () => {
    await app("m=kept&v=nightly&b=on&os=ios");
    expect(await nightly()).toHaveTextContent("Last backup today, 3:12 am · 1.4 GB in your iCloud");
    expect(screen.getByRole("switch", { name: "Wi-Fi only" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(screen.getByRole("button", { name: "Back up now" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change the phrase…" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Make a recovery key….*None made yet/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Turn off…" })).toBeInTheDocument();
    const about = screen.getByRole("region", { name: "About the backup" });
    expect(within(about).getByText("A phone that already has a memory")).toBeInTheDocument();
    expect(within(about).queryByText("Turning it off")).toBeNull();
    expect(
      within(about).getByText(/Sign in with the same Apple Account, install Nocturne, choose/),
    ).toHaveTextContent("choose I have a backup, and type your phrase.");
  });

  it("waits for Wi-Fi, and backs up now over mobile data once when asked", async () => {
    await app("m=kept&v=nightly&b=wifi");
    expect(await nightly()).toHaveTextContent("Last backup Wednesday, 2:40 am · waiting for Wi-Fi");
    expect(
      screen.getByText(
        "Wi-Fi only is on, so the backup waits until this phone is on Wi-Fi and charging.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Back up now over mobile data" }));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: /Back up every night/ })).toHaveTextContent(
        "Last backup today, 9:52 pm",
      ),
    );
  });

  it.each([
    [
      "noRoom",
      "",
      "There’s no room in your Google Drive for tonight’s backup. Your last backup from Monday is still there.",
      "Open Google Drive",
    ],
    [
      "noRoom",
      "&os=ios",
      "There’s no room in your iCloud for tonight’s backup. Your last backup from Monday is still there.",
      "Open iCloud settings",
    ],
    [
      "signedOut",
      "",
      "The Google account your backup uses isn’t on this phone any more, so the backup has stopped.",
      "Choose account",
    ],
    [
      "signedOut",
      "&os=ios",
      "This iPhone is signed out of iCloud, so the backup has stopped. Sign in to iCloud in Settings to carry on.",
      "Open Settings",
    ],
    [
      "noPermission",
      "",
      "Google no longer lets Nocturne back up. Choose the account again.",
      "Choose account",
    ],
    [
      "noPermission",
      "&os=ios",
      "iCloud is turned off for Nocturne, so the backup has stopped. Turn it on in Settings, under iCloud.",
      "Open Settings",
    ],
    [
      "newer",
      "",
      "A newer Nocturne made the latest backup. Update Nocturne from Google Play to carry on.",
      "Update",
    ],
  ])("stopped (%s%s) says why and what to do", async (state, os, line, action) => {
    await app(`m=kept&v=nightly&b=${state}${os}`);
    expect(await nightly()).toHaveTextContent("Stopped · last backup Monday, 3:05 am");
    expect(screen.getByText(line)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: action })).toBeInTheDocument();
  });

  it("on an iPhone, says where to go once Settings opens on Nocturne's page", async () => {
    await app("m=kept&v=nightly&b=noRoom&os=ios");
    expect(
      await screen.findByText(
        "Settings opens on Nocturne’s page. Go back to the first page of Settings, then tap your name, then iCloud.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open iCloud settings" })).toBeInTheDocument();
  });

  it("on an iPhone, asks iCloud again on coming back from Settings", async () => {
    await app("m=kept&v=nightly&b=signedOut&os=ios");
    expect(await screen.findByRole("button", { name: "Open Settings" })).toBeInTheDocument();
    document.dispatchEvent(new Event("visibilitychange"));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Open Settings" })).not.toBeInTheDocument(),
    );
    expect(scriptedBackup.calls).toContain("backup_connect");
  });

  it("choosing another account says where the earlier backup stays", async () => {
    await app("m=kept&v=nightly&b=signedOut");
    fireEvent.click(await screen.findByRole("button", { name: "Choose account" }));
    expect(
      await screen.findByText(
        "Your earlier backup stays in the Drive of maya.old@example.com. Delete it there if you no longer want it.",
      ),
    ).toBeInTheDocument();
  });

  it("another phone's newer backup stops this one until it is made the one in use", async () => {
    await app("m=kept&v=nightly&b=another");
    expect(
      await screen.findByText(
        "Another phone made a newer backup on Thursday 12 November. This phone has stopped backing up, so nothing is overwritten.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Make this phone the one in use" }));
    const ask = await screen.findByRole("dialog", { name: "Make this phone the one in use" });
    expect(
      within(ask).getByText(
        "This phone’s next backup becomes the newest. The other phone stops when it next looks.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(ask).getByRole("button", { name: "Make this phone the one in use" }));
    await waitFor(() => expect(scriptedBackup.calls).toContain("backup_make_in_use"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("turns off keeping the backup, and on again with the same phrase", async () => {
    await app("m=kept&v=nightly&b=on");
    fireEvent.click(await screen.findByRole("button", { name: "Turn off…" }));
    const sheet = await screen.findByRole("dialog", { name: "Turn off backup?" });
    expect(
      within(sheet).getByText(
        "No new backups are made. The backup in your Google Drive stays as it is until you delete it.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Turn off" }));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: /Back up every night/ })).toHaveTextContent(
        "Off · your backup from today is still in your Google Drive",
      ),
    );
    fireEvent.click(screen.getByRole("switch", { name: /Back up every night/ }));
    expect(await screen.findByText("On again, with the same phrase.")).toBeInTheDocument();
    expect(screen.queryByText("Backup · step 1 of 3")).toBeNull();
  });

  it("turns off and deletes the backup, after saying what goes", async () => {
    await app("m=kept&v=nightly&b=on");
    fireEvent.click(await screen.findByRole("button", { name: "Turn off…" }));
    const sheet = await screen.findByRole("dialog", { name: "Turn off backup?" });
    fireEvent.click(within(sheet).getByLabelText("Also delete the backup"));
    expect(
      within(sheet).getByText(
        "All seven nights go from your Google Drive for good. Nobody can bring them back.",
      ),
    ).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Turn off" })).toHaveClass("ph-btn--danger");
    fireEvent.click(within(sheet).getByRole("button", { name: "Turn off" }));
    expect(await screen.findByText("Your backup was deleted.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /Back up every night/ })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    fireEvent.click(screen.getByRole("switch", { name: /Back up every night/ }));
    expect(await screen.findByText("Backup · step 1 of 3")).toBeInTheDocument();
  });

  it("makes a new recovery key, shown once", async () => {
    await app("m=kept&v=nightly&b=on");
    fireEvent.click(await screen.findByRole("button", { name: /Make a recovery key…/ }));
    const ask = await screen.findByRole("dialog", { name: "Make a recovery key?" });
    fireEvent.click(within(ask).getByRole("button", { name: "Make it" }));
    expect(
      await screen.findByRole("heading", { name: "Your new recovery key" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Recovery key" })).toHaveTextContent(
      NEW_KEY.replace(/-/g, ""),
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Print or save…" }));
    fireEvent.click(await screen.findByRole("button", { name: "Done" }));
    expect(
      await screen.findByRole("button", { name: /Make a recovery key….*Made today/ }),
    ).toBeInTheDocument();
  });

  it("changes the phrase after asking, with steps 2 and 3 and Save", async () => {
    await app("m=kept&v=nightly&b=on");
    fireEvent.click(await screen.findByRole("button", { name: "Change the phrase…" }));
    const ask = await screen.findByRole("dialog", { name: "Change the phrase?" });
    expect(
      within(ask).getByText(
        "Your old phrase stops opening the backup. A recovery key you saved still works.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(ask).getByRole("button", { name: "Change it" }));
    expect(await screen.findByText("Backup · step 2 of 3")).toBeInTheDocument();
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.queryByRole("switch", { name: /Wi-Fi only/ })).toBeNull();
    fireEvent.change(await screen.findByLabelText("Your phrase"), {
      target: { value: SCRIPTED_PHRASE },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await nightly()).toBeInTheDocument();
    expect(scriptedBackup.calls).toContain("backup_change_phrase");
  });
});

describe("the backup on Today", () => {
  it("says once that the backup stopped, with no notification", async () => {
    await app("m=kept&b=noRoom");
    const card = await screen.findByRole("region", { name: "Backup" });
    expect(within(card).getByText("Your backup has stopped.")).toBeInTheDocument();
    expect(
      within(card).getByText(
        "There’s no room in your Google Drive for tonight’s backup. Your last backup from Monday is still there.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("region", { name: "Backup" })).toBeNull();
    cleanup();
    clearMocks();
    await app("m=kept&b=noRoom");
    await screen.findByText("Your memory is on this phone");
    await new Promise((settle) => setTimeout(settle, 50));
    expect(screen.queryByRole("region", { name: "Backup" })).toBeNull();
  });

  it("says when the last backup was a week ago, and opens the backup", async () => {
    await app("m=kept&b=late");
    const card = await screen.findByRole("region", { name: "Backup" });
    expect(
      within(card).getByText("Your last backup was Wednesday 4 November."),
    ).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Open backup" }));
    expect(await nightly()).toBeInTheDocument();
  });

  it("offers to turn on backup in place of saving one", async () => {
    await app("m=reminder");
    expect(
      await screen.findByText("No backup yet. If this phone is lost, what you kept goes with it."),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Turn on backup" }));
    expect(await screen.findByText("Backup · step 1 of 3")).toBeInTheDocument();
  });
});

describe("Delete everything with a backup", () => {
  it("says the backup goes too, and Welcome says where it went", async () => {
    await app("m=kept&b=on&s=welcome");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
    expect(
      await screen.findByText(/Your backup in your Google Drive is deleted too\./),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/Type/), { target: { value: "delete everything" } });
    fireEvent.click(screen.getByRole("button", { name: "Delete everything" }));
    expect(
      await screen.findByText("Everything was deleted from this phone and from your Google Drive."),
    ).toBeInTheDocument();
  });

  it("without a backup, says what it always said", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
    expect(
      await screen.findByText(/A backup you saved elsewhere isn’t touched\./),
    ).toBeInTheDocument();
  });
});
