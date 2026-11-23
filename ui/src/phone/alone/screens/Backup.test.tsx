import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BackupProgress, BackupReport } from "../../../engine/bindings";
import { type Checked, HomeRequestError, type Progress } from "../api";
import { Restore, SaveBackup } from "./Backup";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const MB = 1_000_000;
const at = (phase: BackupProgress["phase"], done: number, total: number): BackupProgress => ({
  phase,
  done: done * MB,
  total: total * MB,
});

/** A job the test finishes or reports on by hand. */
function job<T>() {
  let report: Progress = () => undefined;
  let finish: (value: T) => void = () => undefined;
  let fail: (error: unknown) => void = () => undefined;
  const promise = new Promise<T>((resolve, reject) => {
    finish = resolve;
    fail = reject;
  });
  return {
    promise,
    progress: (onProgress: Progress) => {
      report = onProgress;
    },
    report: (p: BackupProgress) => act(() => report(p)),
    finish,
    fail,
  };
}

function fillAndSave() {
  fireEvent.change(screen.getByLabelText("Choose a passphrase"), {
    target: { value: "copper kettle orbit" },
  });
  fireEvent.change(screen.getByLabelText("Type it again"), {
    target: { value: "copper kettle orbit" },
  });
  fireEvent.click(screen.getByLabelText("I’ve written it down or saved it somewhere safe"));
  fireEvent.click(screen.getByRole("button", { name: "Save the backup…" }));
}

describe("making a backup", () => {
  it("shows how far it is and how long is left, and can be stopped", async () => {
    const run = job<BackupReport | null>();
    const stop = vi.fn(async () => {
      run.fail(new HomeRequestError("stopped"));
    });
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    render(
      <SaveBackup
        save={(_pass, onProgress) => {
          run.progress(onProgress);
          return run.promise;
        }}
        stop={stop}
        now={() => 0}
        onClose={() => undefined}
      />,
    );
    fillAndSave();
    expect(await screen.findByRole("heading", { name: "Making your backup" })).toBeVisible();
    expect(screen.getByText("Saving")).toBeVisible();
    expect(
      screen.getByText(
        "You can keep using Nocturne. Anything you keep from now on goes in your next backup.",
      ),
    ).toBeVisible();

    run.report(at("saving", 10, 100));
    expect(screen.getByText(/^10\s?MB of 100\s?MB$/)).toBeVisible();

    now.mockReturnValue(1_002_000);
    run.report(at("saving", 30, 100));
    expect(screen.getByText(/^30\s?MB of 100\s?MB · about 7 seconds left$/)).toBeVisible();

    run.report(at("checking", 5, 100));
    expect(screen.getByText("Checking the backup")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(stop).toHaveBeenCalledOnce();
    expect(await screen.findByRole("heading", { name: "Stopped." })).toBeVisible();
    expect(screen.getByText("Nothing was saved, and nothing in Nocturne changed.")).toBeVisible();
  });
});

describe("bringing a backup back", () => {
  const checked: Checked = {
    check: {
      summary: {
        madeAt: "2026-11-21T09:32:00.000Z",
        appVersion: "0.9.0",
        storeVersion: 1,
        memories: 0,
        items: 3,
        pages: 0,
        originals: 1,
        onlineUsed: false,
        awayUsed: false,
        bytes: 3 * MB,
      },
      here: null,
      putAside: null,
    },
    backup: { things: 3, journalEntries: 1, photosAndRecordings: 1, checkins: 0 },
    here: null,
  };

  function bring(
    restore: (p: Progress) => Promise<void>,
    stop = vi.fn<() => Promise<void>>(async () => undefined),
  ) {
    render(
      <Restore
        pickFile={async () => ({ path: "/file", name: "backup.nocturne", bytes: 3 * MB })}
        check={async () => checked}
        restore={async (_file, _pass, _replace, onProgress) => restore(onProgress)}
        stop={stop}
        letGo={async () => undefined}
        onBack={() => undefined}
        onDone={() => undefined}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Choose the file" }));
    return stop;
  }

  it("can be stopped until the last step, which says it cannot", async () => {
    const run = job<void>();
    bring((onProgress) => {
      run.progress(onProgress);
      return run.promise;
    });
    fireEvent.change(await screen.findByLabelText("Passphrase for this backup"), {
      target: { value: "copper kettle orbit" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    await waitFor(() => expect(screen.getByText("Restoring")).toBeVisible());
    expect(
      screen.getByText(
        "Keep Nocturne open until this finishes. If it closes, nothing on this phone changes and you can start again.",
      ),
    ).toBeVisible();

    run.report(at("restoring", 40, 100));
    expect(screen.getByRole("button", { name: "Stop" })).not.toHaveAttribute("aria-disabled");

    run.report(at("restoring", 100, 100));
    expect(screen.getByText("Almost done. This last step can’t be stopped.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Stop" })).toHaveAttribute("aria-disabled", "true");
  });

  it("says nothing changed when it is stopped", async () => {
    const run = job<void>();
    const stop = bring(
      (onProgress) => {
        run.progress(onProgress);
        return run.promise;
      },
      vi.fn(async () => run.fail(new HomeRequestError("stopped"))),
    );
    fireEvent.change(await screen.findByLabelText("Passphrase for this backup"), {
      target: { value: "copper kettle orbit" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    await waitFor(() => expect(screen.getByText("Restoring")).toBeVisible());
    run.report(at("restoring", 10, 100));
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(stop).toHaveBeenCalledOnce();
    expect(await screen.findByRole("heading", { name: "Stopped." })).toBeVisible();
    expect(screen.getByText("Nothing on this phone changed.")).toBeVisible();
  });
});
