import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import packageJson from "../../../package.json";
import { EngineProvider } from "../../engine/context";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { PlatformProvider } from "../../platform/context";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { RestoreFlow } from "./Restore";

const PASS = "lantern orchard tuesday";
const FILE = "/backups/Nocturne backup 2026-11-15.nocturne";

afterEach(() => {
  window.location.hash = "";
  document.body.removeAttribute("aria-busy");
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function open(engine: MockEngine = createMockEngine(), platform = fakePlatform()) {
  setViewport(1280);
  const view = renderApp({ hash: "#/privacy/restore", engine, platform });
  await screen.findByRole("heading", { level: 1, name: "Restore from a backup" });
  return { ...view, engine, platform };
}

async function pick(platform: ReturnType<typeof fakePlatform>, file = FILE) {
  vi.spyOn(platform, "pickBackup").mockResolvedValue(file);
  await userEvent.click(screen.getByRole("button", { name: "Choose backup file…" }));
  await screen.findByLabelText("Passphrase for this backup");
}

async function fill(passphrase = PASS) {
  await userEvent.type(screen.getByLabelText("Passphrase for this backup"), passphrase);
}

async function openBackup() {
  await userEvent.click(screen.getByRole("button", { name: "Open backup" }));
}

const boxAround = (text: string) => {
  const box = screen.getByText(text).closest('[role="alert"]');
  expect(box).not.toBeNull();
  return box as HTMLElement;
};

describe("Restore from a backup: choosing a file", () => {
  it("says what restoring does, and asks for the file and its passphrase", async () => {
    const engine = createMockEngine();
    const { platform } = await open(engine);
    expect(
      screen.getByText(
        "Bring back everything from a Nocturne backup file. Nothing on this computer changes until you say so.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "What restoring does" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Checks the whole backup before anything changes")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Afterwards" })).toBeInTheDocument();
    expect(
      screen.getByText("stay off until you paste the key for your provider again."),
    ).toBeInTheDocument();
    expect(screen.getByText("stay off until you add your phones again.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose backup file…" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    await pick(platform);
    expect(screen.getByText("Nocturne backup 2026-11-15.nocturne")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose another…" })).toBeInTheDocument();
    const field = screen.getByLabelText("Passphrase for this backup");
    expect(field).toHaveAttribute("type", "password");
    expect(field).toHaveAttribute("autocomplete", "current-password");
    expect(field).not.toHaveAttribute("spellcheck", "true");
    expect(field).toHaveFocus();
    expect(screen.getByRole("checkbox", { name: "Show passphrase" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Open backup" })).toBeInTheDocument();
  });

  it("shows the passphrase only when asked", async () => {
    const engine = createMockEngine();
    const { platform } = await open(engine);
    await pick(platform);
    const show = screen.getByRole("checkbox", { name: "Show passphrase" });
    await userEvent.click(show);
    expect(screen.getByLabelText("Passphrase for this backup")).toHaveAttribute("type", "text");
    await userEvent.click(show);
    expect(screen.getByLabelText("Passphrase for this backup")).toHaveAttribute("type", "password");
  });

  it("keeps the technical rows behind Advanced details", async () => {
    const engine = createMockEngine();
    const { platform } = await open(engine);
    await pick(platform);
    const more = screen.getByRole("button", { name: "Advanced details" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByText("Nocturne backup 2026-11-15.nocturne").length).toBeGreaterThan(1);
    expect(screen.getAllByText("Read once the passphrase opens the file")).toHaveLength(2);
    expect(screen.getByText("Every part is checked before anything changes")).toBeInTheDocument();
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
  });
});

describe("Restore from a backup: the empty computer", () => {
  it("checks the file, says what is in it, restores it, and opens today's page", async () => {
    const engine = createMockEngine();
    engine.script.here = null;
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    expect(
      await screen.findByRole("heading", { level: 1, name: "Ready to restore" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Backup made")).toBeInTheDocument();
    expect(screen.getByText(/memories, .* pages and .* originals/)).toBeInTheDocument();
    expect(screen.getByText("Size")).toBeInTheDocument();
    expect(screen.getByText("Everything is there.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restore" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Restore" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Your memories are back." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/from your backup of \d+ Nov\./)).toBeInTheDocument();
    expect(screen.getByText("Nocturne’s mind")).toBeInTheDocument();
    expect(screen.getByText("Already on this computer. Nothing to do.")).toBeInTheDocument();
    expect(
      screen.getAllByRole("status").some((node) => node.textContent === "Your memories are back."),
    ).toBe(true);
    expect(document.body).not.toHaveAttribute("aria-busy");

    await userEvent.click(screen.getByRole("button", { name: "Open today’s page" }));
    await waitFor(() => expect(window.location.hash).toBe("#/today"));
  });

  it("shows the restored rows for online answers, Away from home and what was put aside", async () => {
    const engine = createMockEngine();
    engine.script.restored = {
      madeAt: "2026-11-02T10:00:00.000Z",
      memories: 12,
      pages: 3,
      originals: 4,
      onlineUsed: true,
      awayUsed: true,
      putAsideAt: "2026-11-15T10:00:00.000Z",
    };
    await open(engine);

    expect(
      await screen.findByRole("heading", { level: 1, name: "Your memories are back." }),
    ).toBeInTheDocument();
    expect(screen.getByText("Online answers")).toBeInTheDocument();
    expect(screen.getByText("Set up online answers")).toHaveAttribute("href", "#/settings/online");
    expect(screen.getByText("Away from home")).toBeInTheDocument();
    expect(screen.getByText("Away from home settings")).toHaveAttribute("href", "#/settings/away");
    expect(screen.getByText("What was here before")).toBeInTheDocument();
    expect(
      screen.getByText("Put aside 15 Nov. Bring it back any time from Privacy."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open today’s page" })).toBeInTheDocument();
  });
});

describe("Restore from a backup: replacing what is here", () => {
  it("shows both sides, with Keep what’s here first, and focus on the title", async () => {
    const engine = createMockEngine();
    engine.script.here = {
      memories: 3,
      pages: 2,
      originals: 1,
      lastKept: "2026-11-14T10:00:00.000Z",
      keptAfterBackup: 4,
      bytes: 2_100_000_000,
    };
    engine.script.putAside = [
      {
        id: "2026-11-01 1010",
        at: "2026-11-01T10:10:00.000Z",
        memories: 8,
        pages: 2,
        originals: 1,
        bytes: 1_000_000,
      },
    ];
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    const heading = await screen.findByRole("heading", {
      level: 1,
      name: "Replace what’s on this computer?",
    });
    expect(heading).toHaveFocus();
    expect(screen.getByRole("heading", { level: 2, name: "In the backup" })).toBeInTheDocument();
    expect(screen.getByText("Comes in")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "On this computer now" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Put aside, not deleted")).toBeInTheDocument();
    expect(screen.getByText("Last kept")).toBeInTheDocument();
    expect(screen.getByText("Kept after the backup")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Something is already put aside from 1 Nov. It stays, and this is put aside as well.",
      ),
    ).toBeInTheDocument();

    const keep = screen.getByRole("button", { name: "Keep what’s here" });
    const replace = screen.getByRole("button", { name: "Replace with the backup" });
    expect(keep.compareDocumentPosition(replace) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps what is here and returns to Privacy, with nothing changed", async () => {
    const engine = createMockEngine();
    const { platform } = await open(engine);
    const restore = vi.spyOn(engine, "restoreBackup");
    await pick(platform);
    await fill();
    await openBackup();
    await screen.findByRole("heading", {
      level: 1,
      name: "Replace what’s on this computer?",
    });

    await userEvent.click(screen.getByRole("button", { name: "Keep what’s here" }));
    await waitFor(() => expect(window.location.hash).toBe("#/privacy"));
    expect(restore).not.toHaveBeenCalled();
    expect(await engine.getRestored()).toBeNull();
  });
});

describe("Restore from a backup: failures", () => {
  it("says a wrong passphrase once, then points at the password manager", async () => {
    const engine = createMockEngine();
    const { platform } = await open(engine);
    await pick(platform);
    await fill("wrong passphrase");
    await openBackup();

    expect(
      await screen.findByText(
        "That passphrase doesn’t open this backup. Check capital letters, and try again.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Passphrase for this backup")).toHaveValue("wrong passphrase");

    await openBackup();
    expect(
      await screen.findByText(
        "That passphrase doesn’t open this backup. Check capital letters, and try again.",
      ),
    ).toBeInTheDocument();

    await openBackup();
    expect(
      await screen.findByText(
        "That passphrase doesn’t open this backup. If you saved it in a password manager, copy it from there.",
      ),
    ).toBeInTheDocument();
  });

  it("says a backup is from a newer Nocturne, with both versions", async () => {
    const engine = createMockEngine();
    engine.script.check.fail = "newerBackup";
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    const box = await waitFor(() => boxAround("This backup is from a newer Nocturne"));
    expect(
      within(box).getByText(
        `It was made with Nocturne 0.4.0, and this computer has ${packageJson.version}. Update Nocturne on this computer, then try again. Nothing was changed.`,
      ),
    ).toBeInTheDocument();
    expect(within(box).getByRole("button", { name: "Choose another file…" })).toHaveFocus();
  });

  it("says a file is not a Nocturne backup, and changes nothing", async () => {
    const engine = createMockEngine();
    engine.script.probe.fail = "notABackup";
    const { platform } = await open(engine);
    vi.spyOn(platform, "pickBackup").mockResolvedValue("/notes/words.txt");
    await userEvent.click(screen.getByRole("button", { name: "Choose backup file…" }));

    const box = await waitFor(() => boxAround("This file can’t be restored"));
    expect(
      within(box).getByText(
        "It isn’t a Nocturne backup, or it’s damaged. Nothing was changed. If you have another copy, try that one.",
      ),
    ).toBeInTheDocument();
    expect(within(box).getByRole("button", { name: "Choose another file…" })).toHaveFocus();
    expect(screen.queryByLabelText("Passphrase for this backup")).toBeNull();
  });

  it("says a damaged part cannot be restored safely", async () => {
    const engine = createMockEngine();
    engine.script.check.fail = "damagedBackup";
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    expect(
      await screen.findByRole("heading", { level: 1, name: "Checking the backup" }),
    ).toBeInTheDocument();
    const box = await waitFor(() => boxAround("Part of this backup is damaged"));
    expect(
      within(box).getByText(
        "Nocturne can’t restore it safely, so nothing was changed. If you have another copy, try that one.",
      ),
    ).toBeInTheDocument();
    expect(within(box).getByRole("button", { name: "Choose another file…" })).toHaveFocus();
  });

  it("says there is not enough room to restore", async () => {
    const engine = createMockEngine();
    engine.script.check.fail = "noRoom";
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    const box = await waitFor(() => boxAround("There isn’t room to restore"));
    expect(within(box).getByRole("button", { name: "Check again" })).toHaveFocus();
  });

  it("says when the backup file went away, and offers to try again", async () => {
    const engine = createMockEngine();
    engine.script.check.fail = "notFound";
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    const box = await waitFor(() => boxAround("The backup file went away"));
    expect(
      within(box).getByText(
        "Nocturne couldn’t finish reading it, so nothing was changed. If it’s on a drive, plug it back in and try again.",
      ),
    ).toBeInTheDocument();
    expect(within(box).getByRole("button", { name: "Try again" })).toHaveFocus();
  });

  it("stops at once while checking, and keeps the file and passphrase", async () => {
    const engine = createMockEngine();
    engine.script.check.steps = [0.2, 0.4, 0.6, 0.8];
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();

    expect(await screen.findByRole("progressbar", { name: "Checking" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(await screen.findByLabelText("Passphrase for this backup")).toHaveValue(PASS);
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.getByText("Nocturne backup 2026-11-15.nocturne")).toBeInTheDocument();
  });
});

describe("Restore from a backup: restoring", () => {
  it("shows the last step cannot be stopped, then the restored card", async () => {
    const engine = createMockEngine();
    engine.script.here = null;
    engine.script.restore.steps = [0.5, 1];
    engine.script.restore.holdFinalMs = 300;
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();
    await screen.findByRole("heading", { level: 1, name: "Ready to restore" });
    await userEvent.click(screen.getByRole("button", { name: "Restore" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Restoring your memories" }),
    ).toBeInTheDocument();
    expect(document.body).toHaveAttribute("aria-busy", "true");
    const final = await screen.findByRole("progressbar", { name: "Almost done" });
    expect(final).toBeInTheDocument();
    const stop = screen.getByRole("button", { name: "Stop" });
    expect(stop).toBeDisabled();
    expect(screen.getByText("Almost done. This last step can’t be stopped.")).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { level: 1, name: "Your memories are back." }),
    ).toBeInTheDocument();
    expect(document.body).not.toHaveAttribute("aria-busy");
  });

  it("stops while restoring, changes nothing, and offers to restore again", async () => {
    const engine = createMockEngine();
    engine.script.here = null;
    engine.script.restore.steps = [0.2, 0.4, 0.6, 0.8];
    const { platform } = await open(engine);
    await pick(platform);
    await fill();
    await openBackup();
    await screen.findByRole("heading", { level: 1, name: "Ready to restore" });
    await userEvent.click(screen.getByRole("button", { name: "Restore" }));

    expect(await screen.findByRole("progressbar", { name: "Restoring" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Ready to restore" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Stopped. Nothing on this computer changed.")).toBeInTheDocument();
    expect(await engine.getRestored()).toBeNull();
    expect(screen.getByRole("button", { name: "Restore" })).toBeEnabled();
  });
});

describe("Restore from a backup: in setup", () => {
  it("uses the setup words, no links, and Continue", async () => {
    const engine = createMockEngine();
    engine.script.restored = {
      madeAt: "2026-11-02T10:00:00.000Z",
      memories: 12,
      pages: 3,
      originals: 4,
      onlineUsed: true,
      awayUsed: true,
      putAsideAt: null,
    };
    const onContinue = vi.fn();
    render(
      <EngineProvider engine={engine}>
        <PlatformProvider platform={fakePlatform()}>
          <RestoreFlow mode="setup" onContinue={onContinue} />
        </PlatformProvider>
      </EngineProvider>,
    );

    expect(
      await screen.findByRole("heading", { level: 1, name: "Your memories are back." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Downloads next, during setup (8.4 GB). Your memories and pages are here meanwhile.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Off until you paste the key for your provider again. You can do that in Settings later.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Set up online answers")).toBeNull();
    expect(screen.queryByText("Away from home settings")).toBeNull();
    expect(screen.queryByRole("link", { name: "Privacy" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 2, name: "What restoring does" })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(onContinue).toHaveBeenCalled();
  });
});
