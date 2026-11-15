import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BackupsView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";

const EMPTY: BackupsView = {
  last: null,
  putAside: [],
  running: null,
  interruptedBackup: false,
  interruptedRestore: false,
};

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

async function open(engine = createMockEngine({ sentMeasured: true })) {
  setViewport(1280);
  const view = renderApp({ hash: "#/privacy", engine, platform: fakePlatform() });
  await screen.findByRole("heading", { level: 1, name: "Nothing." });
  return { ...view, engine };
}

const scripted = (engine: ReturnType<typeof createMockEngine>, over: Partial<BackupsView>) =>
  vi.spyOn(engine, "getBackups").mockResolvedValue({ ...EMPTY, ...over });

describe("the Backup block on Privacy", () => {
  it("sits between Export everything and Delete everything, and opens the Make screen", async () => {
    await open();
    const blocks = [...document.querySelectorAll("section")].map(
      (section) => section.querySelector("h2")?.textContent ?? "",
    );
    expect(blocks.indexOf("Back up to one file")).toBeGreaterThan(
      blocks.indexOf("Export everything"),
    );
    expect(blocks.indexOf("Back up to one file")).toBeLessThan(blocks.indexOf("Delete everything"));
    expect(
      screen.getByText(
        "Everything in one file, locked with a passphrase you choose. Use it to keep a copy somewhere safe, or to move to a new computer.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restore from a backup…" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Make a backup…" }));
    await waitFor(() => expect(window.location.hash).toBe("#/privacy/backup"));
  });

  it("opens the Restore screen from the quiet button", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Restore from a backup…" }));
    await waitFor(() => expect(window.location.hash).toBe("#/privacy/restore"));
  });

  it("says when the last backup was, and how big", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    scripted(engine, {
      last: { at: "2026-11-02T09:00:00.000Z", bytes: 4_100_000_000 },
    });
    await open(engine);
    expect(await screen.findByText("Last backup 2 Nov, 4.1 GB.")).toBeInTheDocument();
  });

  it("gives way to the progress of a running backup, with a way to see it", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    scripted(engine, {
      running: { phase: "saving", done: 1_600_000_000, total: 4_300_000_000 },
    });
    await open(engine);
    expect(
      await screen.findByRole("progressbar", { name: "Making your backup" }),
    ).toBeInTheDocument();
    expect(screen.getByText("1.6 GB of 4.3 GB")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Make a backup…" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Restore from a backup…" })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "See progress" }));
    await waitFor(() => expect(window.location.hash).toBe("#/privacy/backup"));
  });

  it("follows a backup that is running now, then shows the last one", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    const run = engine.makeBackup("/backups/today.nocturne", "lantern orchard tuesday");
    await open(engine);
    expect(
      await screen.findByRole("progressbar", { name: "Making your backup" }),
    ).toBeInTheDocument();
    await run;
    expect(
      await screen.findByText(/^Last backup \d+ Nov, 4\.3 GB\.$/, {}, { timeout: 3000 }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.getByRole("button", { name: "Make a backup…" })).toBeInTheDocument();
  });

  it("says when the last backup did not finish, and clears that when the person acts", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    scripted(engine, { interruptedBackup: true });
    const dismiss = vi.spyOn(engine, "dismissBackupNotices");
    await open(engine);
    expect(
      await screen.findByText("Your last backup didn’t finish, so it wasn’t saved."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Make a backup" }));
    expect(dismiss).toHaveBeenCalled();
    await waitFor(() => expect(window.location.hash).toBe("#/privacy/backup"));
  });

  it("lists every copy put aside, with its day, memories and size", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    engine.script.putAside = [
      {
        id: "2026-11-15 101010",
        at: "2026-11-15T10:10:10.000Z",
        memories: 312,
        pages: 20,
        originals: 140,
        bytes: 2_100_000_000,
      },
      {
        id: "2026-11-02 090000",
        at: "2026-11-02T09:00:00.000Z",
        memories: 1,
        pages: 1,
        originals: 0,
        bytes: 120_000_000,
      },
    ];
    await open(engine);
    expect(await screen.findAllByText("Put aside before restoring")).toHaveLength(2);
    expect(screen.getByText("15 Nov · 312 memories · 2.1 GB")).toBeInTheDocument();
    expect(screen.getByText("2 Nov · 1 memory · 120 MB")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Bring it back…" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Delete it…" })).toHaveLength(2);
  });

  it("asks before bringing a copy back, then shows the bringing-back card", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    const copy = {
      id: "2026-11-15 101010",
      at: "2026-11-15T10:10:10.000Z",
      memories: 312,
      pages: 20,
      originals: 140,
      bytes: 2_100_000_000,
    };
    engine.script.putAside = [copy];
    engine.script.bringBack.steps = [0.5, 1];
    engine.script.bringBack.holdFinalMs = 200;
    const restorePutAside = vi.spyOn(engine, "restorePutAside");
    await open(engine);

    await userEvent.click(await screen.findByRole("button", { name: "Bring it back…" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Go back to what was here before?",
    });
    expect(
      within(dialog).getByText(
        "What you restored from the backup is put aside instead, so you can still change your mind.",
      ),
    ).toBeInTheDocument();
    const notNow = within(dialog).getByRole("button", { name: "Not now" });
    expect(notNow).toHaveFocus();
    expect(within(dialog).getByRole("button", { name: "Bring it back" })).toHaveAttribute(
      "data-variant",
      "primary",
    );
    await userEvent.click(notNow);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(restorePutAside).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Bring it back…" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Bring it back" }),
    );
    expect(
      await screen.findByRole("heading", { level: 1, name: "Bringing back what was here" }),
    ).toBeInTheDocument();
    expect(document.body).toHaveAttribute("aria-busy", "true");
    expect(await screen.findByRole("progressbar", { name: "Restoring" })).toBeInTheDocument();
    expect(restorePutAside).toHaveBeenCalledWith(copy.id);
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { level: 1, name: "Bringing back what was here" }),
      ).toBeNull(),
    );
    expect(document.body).not.toHaveAttribute("aria-busy");
  });

  it("asks before deleting a copy, and deletes only on the second press", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    const copy = {
      id: "2026-11-15 101010",
      at: "2026-11-15T10:10:10.000Z",
      memories: 312,
      pages: 20,
      originals: 140,
      bytes: 2_100_000_000,
    };
    engine.script.putAside = [copy];
    const deletePutAside = vi.spyOn(engine, "deletePutAside");
    await open(engine);

    await userEvent.click(await screen.findByRole("button", { name: "Delete it…" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Delete what was put aside?",
    });
    expect(
      within(dialog).getByText(
        "312 memories, 20 pages and 140 originals from before 15 Nov. This can’t be undone.",
      ),
    ).toBeInTheDocument();
    const keep = within(dialog).getByRole("button", { name: "Keep it" });
    expect(keep).toHaveFocus();
    const confirm = within(dialog).getByRole("button", { name: "Delete it" });
    expect(confirm).toHaveAttribute("data-variant", "forget");
    expect(confirm).toHaveAttribute("data-state", "confirming");
    await userEvent.click(keep);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(deletePutAside).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Delete it…" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete it" }),
    );
    expect(deletePutAside).toHaveBeenCalledWith(copy.id);
    await waitFor(() => expect(screen.queryByText("15 Nov · 312 memories · 2.1 GB")).toBeNull());
  });

  it("says when a restore did not finish, and clears that when the person acts", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    engine.script.interruptedRestore = true;
    const dismiss = vi.spyOn(engine, "dismissBackupNotices");
    await open(engine);
    expect(
      await screen.findByText(
        "Restoring didn’t finish, so Nocturne kept what was here. You can try again from Privacy.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restore from a backup…" }));
    expect(dismiss).toHaveBeenCalled();
    await waitFor(() => expect(window.location.hash).toBe("#/privacy/restore"));
  });

  it("says when a restore finished", async () => {
    const engine = createMockEngine({ sentMeasured: true });
    engine.script.interruptedRestore = true;
    engine.script.restored = {
      madeAt: "2026-11-02T10:00:00.000Z",
      memories: 12,
      pages: 3,
      originals: 4,
      onlineUsed: false,
      awayUsed: false,
      putAsideAt: null,
    };
    await open(engine);
    expect(await screen.findByText("Nocturne finished restoring your backup.")).toBeInTheDocument();
    expect(
      screen.queryByText(
        "Restoring didn’t finish, so Nocturne kept what was here. You can try again from Privacy.",
      ),
    ).toBeNull();
  });
});
