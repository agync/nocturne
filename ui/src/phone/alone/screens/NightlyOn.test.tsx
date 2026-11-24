import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { NativeRoot } from "../../native/NativeRoot";
import { nativeMockServices } from "../../testing/nativeMock";
import { SCRIPTED_PHRASE, scriptedBackup } from "../testing/backupMock";
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

const turnOnSwitch = () => screen.findByRole("switch", { name: /Back up every night/ });

async function toStep2() {
  fireEvent.click(await turnOnSwitch());
  expect(await screen.findByText("Backup · step 1 of 3")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  expect(await screen.findByText("Backup · step 2 of 3")).toBeInTheDocument();
}

describe("turning the backup on", () => {
  it("says why, what goes and what is needed, with everything behind the expander", async () => {
    await app("m=kept&v=nightly");
    fireEvent.click(await turnOnSwitch());
    expect(
      await screen.findByRole("heading", {
        name: "Back up your memory every night, so a lost phone doesn’t lose it.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Only someone with your phrase. Google keeps it in your Drive but can’t open it, and neither can Nocturne’s makers.",
      ),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(
        "Your Google account on this phone, with room for about 412 MB in its Drive.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Nothing is turned on until the last step.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Everything about the backup" }));
    expect(screen.getByText("What the company sees")).toBeInTheDocument();
    expect(
      screen.getByText(/Google can see that the backup is there, how big it is/),
    ).toBeInTheDocument();
    expect(screen.getByText("Turning it off")).toBeInTheDocument();
  });

  it("says when Google didn't allow it, and asks again with Continue", async () => {
    await app("m=kept&v=nightly&connect=refused");
    fireEvent.click(await turnOnSwitch());
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(
      await screen.findByText("Nothing was turned on. Google didn’t allow it."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).not.toHaveAttribute("aria-disabled");
  });

  it("says when the Drive has no room for it", async () => {
    await app("m=kept&v=nightly&connect=full");
    fireEvent.click(await turnOnSwitch());
    await screen.findByText(/room for about 412 MB/);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(
      await screen.findByText(
        "Your Google Drive has 300 MB free. Make room there first, then choose Continue.",
      ),
    ).toBeInTheDocument();
  });

  it("on an iPhone, waits for iCloud before Continue", async () => {
    await app("m=kept&v=nightly&os=ios&connect=turnedOff");
    fireEvent.click(await turnOnSwitch());
    expect(
      await screen.findByText("Turn on iCloud for Nocturne in Settings to back up."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("shows Nocturne's phrase word by word, another on asking, and warns of an easy own one", async () => {
    await app("m=kept&v=nightly");
    await toStep2();
    const words = await screen.findByRole("list", { name: "Your phrase" });
    await waitFor(() =>
      expect(
        within(words)
          .getAllByRole("listitem")
          .map((w) => w.textContent),
      ).toEqual(SCRIPTED_PHRASE.split(" ")),
    );
    expect(within(words).getByLabelText("word 3, pencil")).toBeInTheDocument();
    expect(screen.getByText("Keep this phrase safe")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Another phrase" }));
    await waitFor(() => expect(words).toHaveTextContent("copper"));

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() =>
      expect(scriptedBackup.copied).toBe("copper lantern meadow ribbon quiet anchor"),
    );

    fireEvent.click(screen.getByRole("button", { name: "Use my own phrase instead" }));
    const own = screen.getByLabelText("Your phrase");
    expect(own).toHaveAttribute("autocorrect", "off");
    expect(own).toHaveAttribute("spellcheck", "false");
    expect(own).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "Next" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText("Type a phrase to go on.")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Your phrase"), {
      target: { value: "sunshine garden" },
    });
    expect(screen.getByText(/This phrase is easy to guess/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Use Nocturne’s phrase" }));
    expect(await screen.findByRole("list", { name: "Your phrase" })).toHaveTextContent("copper");
  });

  it("never steps Back into the steps once it is on", async () => {
    await app("m=kept&v=nightly");
    await toStep2();
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.change(await screen.findByLabelText("Your phrase"), {
      target: { value: SCRIPTED_PHRASE },
    });
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(
      await screen.findByRole("heading", { name: "A recovery key, if you want one" }),
    ).toBeInTheDocument();
    history.back();
    expect(await screen.findByRole("switch", { name: /Back up every night/ })).toBeInTheDocument();
    expect(screen.queryByText(/Backup · step/)).toBeNull();
  });

  it("checks the phrase typed back, then turns on, offers a key and backs up", async () => {
    await app("m=kept&v=nightly");
    await toStep2();
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByRole("heading", { name: "Type your phrase" })).toBeInTheDocument();
    expect(screen.getByText("Type your phrase to go on.")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Backups use mobile data too: about 412 MB the first time, then a little each night.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: /Wi-Fi only/ }));
    expect(screen.getByText("Backups wait for Wi-Fi.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: /Wi-Fi only/ }));

    fireEvent.change(screen.getByLabelText("Your phrase"), {
      target: { value: "harbour violet pencil" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(
      await screen.findByText(
        "That isn’t the phrase. Check each word, or go back to see it again.",
      ),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Your phrase"), {
      target: { value: "  Harbour violet PENCIL maple   drift oven " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));

    expect(
      await screen.findByRole("heading", { name: "A recovery key, if you want one" }),
    ).toBeInTheDocument();
    expect(scriptedBackup.phrase).toBe(SCRIPTED_PHRASE);
    expect(screen.getByLabelText("group 1, K7QM")).toBeInTheDocument();
    expect(screen.getByText("Nocturne shows this only once.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(scriptedBackup.calls).toContain("backup_drop_recovery_key"));

    expect(
      await screen.findByRole("heading", { name: "Your memory is backed up." }),
    ).toBeInTheDocument();
    expect(screen.getByText("9:52 pm, in your Google Drive.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    const on = await turnOnSwitch();
    await waitFor(() => expect(on).toHaveAttribute("aria-checked", "true"));
    expect(on).toHaveTextContent("Last backup today, 9:52 pm · 1.4 GB in your Google Drive");
  });

  it("keeps a key that was printed or saved, and says when it was made", async () => {
    await app("m=kept&v=nightly");
    await toStep2();
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.change(await screen.findByLabelText("Your phrase"), {
      target: { value: SCRIPTED_PHRASE },
    });
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    fireEvent.click(await screen.findByRole("button", { name: "Print or save…" }));
    fireEvent.click(await screen.findByRole("button", { name: "Done" }));
    expect(scriptedBackup.calls).not.toContain("backup_drop_recovery_key");
    fireEvent.click(await screen.findByRole("button", { name: "OK" }));
    expect(await screen.findByRole("button", { name: /Make a recovery key….*Made today/ }));
  });

  it("shows the first backup's progress while it runs, and lets the person go on", async () => {
    await app("m=kept&v=nightly&first=slow");
    await toStep2();
    await screen.findByLabelText("word 1, harbour");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.change(await screen.findByLabelText("Your phrase"), {
      target: { value: SCRIPTED_PHRASE },
    });
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
    expect(
      await screen.findByRole("heading", { name: "Backing up for the first time" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("540 MB of 1.4 GB")).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar").getAttribute("aria-valuetext")?.replace(/\s+/gu, " "),
    ).toBe("540 MB of 1.4 GB");
    fireEvent.click(screen.getByRole("button", { name: "Use Nocturne" }));
    expect(await turnOnSwitch()).toBeInTheDocument();
  });
});
