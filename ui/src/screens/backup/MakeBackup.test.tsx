import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BackupReport } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { chipsIn, sentence } from "../../test/sentence";

const PASS = "lantern orchard tuesday";
const BACKUP = "/backups/Nocturne backup 2026-11-15.nocturne";

afterEach(() => {
  window.location.hash = "";
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function open(engine: MockEngine = createMockEngine(), platform = fakePlatform()) {
  setViewport(1280);
  const view = renderApp({ hash: "#/privacy/backup", engine, platform });
  await screen.findByRole("heading", { level: 1, name: "Make a backup" });
  return { ...view, engine, platform };
}

async function fill(passphrase = PASS) {
  await userEvent.type(screen.getByLabelText("Choose a passphrase"), passphrase);
  await userEvent.type(screen.getByLabelText("Type it again"), passphrase);
  await userEvent.click(
    screen.getByRole("checkbox", { name: "I’ve written it down or saved it somewhere safe" }),
  );
}

const save = async () => userEvent.click(screen.getByRole("button", { name: "Save backup…" }));

const passphraseNotShown = () => {
  expect(document.body.textContent ?? "").not.toContain(PASS);
  for (let i = 0; i < localStorage.length; i += 1) {
    const key = localStorage.key(i) ?? "";
    expect(key).not.toContain(PASS);
    expect(localStorage.getItem(key) ?? "").not.toContain(PASS);
  }
};

describe("Make a backup: the form", () => {
  it("asks for a passphrase, warns once, and says why Save is disabled", async () => {
    await open();
    expect(screen.getByLabelText("Choose a passphrase")).toHaveAttribute("type", "password");
    expect(screen.getByLabelText("Choose a passphrase")).toHaveAttribute(
      "autocomplete",
      "new-password",
    );
    expect(screen.getByLabelText("Choose a passphrase")).not.toHaveAttribute("spellcheck", "true");
    expect(
      screen.getByText(/At least 8 characters\. A few words that don’t go together/),
    ).toBeInTheDocument();
    expect(screen.getByText("Keep this passphrase safe")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "In the backup" })).toBeInTheDocument();
    expect(screen.getByText("Everything you kept, exactly as kept")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "Not in the backup" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Nocturne’s mind.")).toBeInTheDocument();
    expect(screen.queryByText("The key for your provider.")).toBeNull();
    expect(screen.queryByText("Phones you added.")).toBeNull();

    const disabled = screen.getByRole("button", { name: "Save backup…" });
    expect(disabled).toBeDisabled();
    const hint = screen.getByText("Choose a passphrase, type it again and tick the box to go on.");
    expect(disabled).toHaveAttribute("aria-describedby", hint.id);
  });

  it("names the online key and the phones only when there is one to leave out", async () => {
    const engine = createMockEngine({
      remote: { scenario: "on", auto: false },
      sentMeasured: true,
    });
    engine.online.setScenario("on");
    await open(engine);
    expect(await screen.findByText("The key for your provider.")).toBeInTheDocument();
    expect(screen.getByText("Phones you added.")).toBeInTheDocument();
    const again = screen.getByText(
      sentence("You add them again in Settings, under Away from home."),
    );
    expect(chipsIn(again)).toEqual(["Settings", "Away from home"]);
  });

  it("keeps the technical rows behind Advanced details, and says what is left out", async () => {
    await open();
    const more = screen.getByRole("button", { name: "Advanced details" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(more.firstElementChild).toHaveAttribute("data-chevron");
    expect(more).not.toHaveAttribute("aria-controls");
    await userEvent.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    expect(document.getElementById(more.getAttribute("aria-controls") ?? "")).not.toBeNull();
    expect(
      screen.getByText("A .nocturne file, written as a .partial file until complete"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "an age file, locked with a key derived from the passphrase by scrypt; the passphrase is never stored",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "engine/ (models), the online answers key, Away from home device keys, logs",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText("Documents/Nocturne/put-aside/")).toBeInTheDocument();
  });

  it("shows both passphrases only when asked", async () => {
    await open();
    const show = screen.getByRole("checkbox", { name: "Show passphrases" });
    await userEvent.click(show);
    expect(screen.getByLabelText("Choose a passphrase")).toHaveAttribute("type", "text");
    expect(screen.getByLabelText("Type it again")).toHaveAttribute("type", "text");
    await userEvent.click(show);
    expect(screen.getByLabelText("Choose a passphrase")).toHaveAttribute("type", "password");
  });

  it("asks for eight characters in the hint, and keeps Save disabled", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("Choose a passphrase"), "short");
    await userEvent.type(screen.getByLabelText("Type it again"), "short");
    await userEvent.click(
      screen.getByRole("checkbox", { name: "I’ve written it down or saved it somewhere safe" }),
    );
    expect(await screen.findByText("Use at least 8 characters.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save backup…" })).toBeDisabled();
  });

  it("speaks the strength in words after a pause, and hides the marks", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("Choose a passphrase"), PASS);
    const words = await screen.findByText("Strong.", {}, { timeout: 2000 });
    expect(words).toHaveAttribute("aria-live", "polite");
    const marks = words.parentElement?.querySelector("[class*=marks]");
    expect(marks).toHaveAttribute("aria-hidden", "true");
    expect(marks).toHaveAttribute("data-level", "3");
  });

  it("says when the two passphrases differ, when the second field loses focus", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("Choose a passphrase"), PASS);
    await userEvent.type(screen.getByLabelText("Type it again"), "lantern orchard tues");
    await userEvent.tab();
    expect(screen.getByText("The two passphrases don’t match.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save backup…" })).toBeDisabled();
    await userEvent.clear(screen.getByLabelText("Type it again"));
    await userEvent.type(screen.getByLabelText("Type it again"), PASS);
    expect(screen.queryByText("The two passphrases don’t match.")).toBeNull();
  });

  it("says the size and where to save once the form is complete", async () => {
    await open();
    await fill();
    expect(
      await screen.findByText("About 4.3 GB. Next, choose where to save it."),
    ).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Save backup…" });
    expect(button).toBeEnabled();
    expect(button).not.toHaveAttribute("aria-describedby");
  });

  it("does nothing when the save dialog is cancelled", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(null);
    const { engine } = await open(createMockEngine(), platform);
    await fill();
    const spy = vi.spyOn(engine, "makeBackup");
    await save();
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { level: 1, name: "Make a backup" })).toBeInTheDocument();
  });
});

describe("Make a backup: a run", () => {
  it("saves through saving and checking to the saved state, then opens the folder", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
    const openPath = vi.spyOn(platform, "openPath");
    const { engine } = await open(createMockEngine(), platform);
    const spy = vi.spyOn(engine, "makeBackup");
    await fill();
    await save();

    expect(spy).toHaveBeenCalledWith(BACKUP, PASS);
    const running = await screen.findByRole("heading", { level: 1, name: "Making your backup" });
    expect(running).toBeInTheDocument();
    expect(running).toHaveFocus();
    expect(await screen.findByText("Making your backup.")).toBeInTheDocument();
    expect(await screen.findByRole("progressbar", { name: "Saving" })).toBeInTheDocument();
    expect(screen.getByText(`Saving to ${BACKUP}`)).toBeInTheDocument();
    expect(
      screen.getByText("If you’re saving to a drive, leave it plugged in until this finishes."),
    ).toBeInTheDocument();

    expect(
      await screen.findByRole("progressbar", { name: "Checking the backup" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Checking the backup.")).toBeInTheDocument();

    expect(
      await screen.findByRole("heading", { level: 1, name: "Your backup is saved." }),
    ).toBeInTheDocument();
    expect(screen.getByText("Nocturne backup 2026-11-15.nocturne")).toBeInTheDocument();
    expect(screen.getByText("/backups")).toBeInTheDocument();
    expect(
      screen.getByText(/^4\.3 GB · \d+ memories, \d+ pages and \d+ originals$/),
    ).toBeInTheDocument();
    expect(screen.getByText("It opens with your passphrase.")).toBeInTheDocument();
    const another = screen.getByText(
      sentence(
        "On another computer, install Nocturne and choose Restore from a backup. You’ll need the passphrase.",
      ),
    );
    expect(chipsIn(another)).toEqual(["Restore from a backup"]);
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
    passphraseNotShown();

    await userEvent.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(openPath).toHaveBeenCalledWith("/backups");

    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(window.location.hash).toBe("#/privacy"));
  });
});

describe("Make a backup: a later visit", () => {
  it("forgets a finished backup with the screen, so the next visit starts fresh", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
    const engine = createMockEngine({ sentMeasured: true });
    await open(engine, platform);
    await fill();
    await save();
    await screen.findByRole("heading", { level: 1, name: "Your backup is saved." });

    window.location.hash = "#/privacy";
    await screen.findByRole("heading", { level: 1, name: "Nothing." });
    window.location.hash = "#/privacy/backup";
    await screen.findByRole("heading", { level: 1, name: "Make a backup" });
    expect(screen.getByLabelText("Choose a passphrase")).toHaveValue("");
    expect(
      screen.getByRole("checkbox", { name: "I’ve written it down or saved it somewhere safe" }),
    ).not.toBeChecked();
  });
});

describe("Make a backup: stopped and failed", () => {
  it("stops at once and keeps the passphrases and the tick", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
    const engine = createMockEngine();
    let rejectRun: (error: unknown) => void = () => undefined;
    vi.spyOn(engine, "makeBackup").mockImplementation(
      () =>
        new Promise((_, reject) => {
          rejectRun = reject;
        }),
    );
    vi.spyOn(engine, "cancelBackup").mockImplementation(async () => {
      rejectRun(new EngineError("cancelled", "stopped"));
    });
    await open(engine, platform);
    await fill();
    await save();
    await screen.findByRole("heading", { level: 1, name: "Making your backup" });
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Make a backup" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Stopped.")).toBeInTheDocument();
    expect(
      screen.getByText("Nothing was saved, and nothing in Nocturne changed."),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Choose a passphrase")).toHaveValue(PASS);
    expect(screen.getByLabelText("Type it again")).toHaveValue(PASS);
    expect(
      screen.getByRole("checkbox", { name: "I’ve written it down or saved it somewhere safe" }),
    ).toBeChecked();
    expect(screen.getByRole("button", { name: "Save backup…" })).toBeEnabled();
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
    passphraseNotShown();
  });

  const failures = [
    {
      code: "noRoom",
      title: "The backup stopped",
      body: /The drive filled up before the backup finished/,
      action: "Choose another place…",
      quiet: false,
    },
    {
      code: "cantWrite",
      title: "Nocturne can’t save there",
      body: /That folder doesn’t let Nocturne save files/,
      action: "Choose another place…",
      quiet: false,
    },
    {
      code: "notFound",
      title: "The backup stopped",
      body: /The drive was disconnected before the backup finished/,
      action: "Try again",
      quiet: true,
    },
    {
      code: "damagedBackup",
      title: "The backup didn’t check out",
      body: /The file was saved but didn’t read back correctly/,
      action: "Try again",
      quiet: false,
    },
    {
      code: "someOddFailure",
      title: "The backup stopped",
      body: /It didn’t finish, so it wasn’t saved/,
      action: "Try again",
      quiet: false,
    },
  ] as const;

  for (const failure of failures) {
    it(`shows ${failure.code} as its own box, and focuses its button`, async () => {
      const platform = fakePlatform();
      vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
      const engine = createMockEngine();
      vi.spyOn(engine, "makeBackup").mockRejectedValue(new EngineError(failure.code, "no"));
      await open(engine, platform);
      await fill();
      await save();

      const title = await screen.findByText(failure.title);
      const box = title.closest('[role="alert"]') as HTMLElement;
      expect(box).not.toBeNull();
      expect(within(box).getByText(failure.body)).toBeInTheDocument();
      const action = within(box).getByRole("button", { name: failure.action });
      expect(action).toHaveFocus();
      if (failure.quiet) {
        expect(
          within(box).getByRole("button", { name: "Choose another place…" }),
        ).toBeInTheDocument();
      } else {
        expect(within(box).getAllByRole("button")).toHaveLength(1);
      }
      expect(screen.queryByRole("button", { name: "Save backup…" })).toBeNull();
      expect(screen.getByLabelText("Choose a passphrase")).toHaveValue(PASS);
      passphraseNotShown();
    });
  }

  it("tries again with the same place, and asks for a new place when told to", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "makeBackup").mockRejectedValue(new EngineError("notFound", "no"));
    await open(engine, platform);
    await fill();
    await save();
    await userEvent.click(await screen.findByRole("button", { name: "Try again" }));
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenLastCalledWith(BACKUP, PASS);

    vi.spyOn(platform, "pickBackupSave").mockResolvedValue("/backups/other.nocturne");
    await userEvent.click(await screen.findByRole("button", { name: "Choose another place…" }));
    expect(spy).toHaveBeenCalledTimes(3);
    expect(spy).toHaveBeenLastCalledWith("/backups/other.nocturne", PASS);
  });
});

describe("Make a backup: leaving and coming back", () => {
  it("keeps running while the person is on another screen, and shows the saved state", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickBackupSave").mockResolvedValue(BACKUP);
    const engine = createMockEngine({ sentMeasured: true });
    let finish: (report: BackupReport) => void = () => undefined;
    vi.spyOn(engine, "makeBackup").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await open(engine, platform);
    await fill();
    await save();
    await screen.findByRole("heading", { level: 1, name: "Making your backup" });

    window.location.hash = "#/privacy";
    await screen.findByRole("heading", { level: 1, name: "Nothing." });
    window.location.hash = "#/privacy/backup";
    expect(
      await screen.findByRole("heading", { level: 1, name: "Making your backup" }),
    ).toBeInTheDocument();

    await act(async () => {
      finish({
        path: BACKUP,
        memories: 14,
        pages: 5,
        originals: 8,
        bytes: 4_300_000_000,
      });
    });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Your backup is saved." }),
    ).toBeInTheDocument();
  });

  it("shows a backup that was already running when the screen opened", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    engine.script.backup.running = { phase: "saving", done: 1_000_000, total: 4_300_000_000 };
    await open(engine);
    expect(
      await screen.findByRole("heading", { level: 1, name: "Making your backup" }),
    ).toBeInTheDocument();
    expect(screen.getByText("1 MB of 4.3 GB")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
  });
});
