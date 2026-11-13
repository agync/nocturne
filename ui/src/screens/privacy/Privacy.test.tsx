import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PrivacyView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const open = async (engine = createMockEngine(), platform = fakePlatform()) => {
  const view = renderApp({ hash: "#/privacy", engine, platform });
  await screen.findByRole("heading", { level: 1, name: "Privacy" });
  await screen.findByText("Sent to the internet since setup");
  return { ...view, engine, platform };
};

const view = async (engine: ReturnType<typeof createMockEngine>, over: Partial<PrivacyView>) => {
  const base = await engine.getPrivacy();
  vi.spyOn(engine, "getPrivacy").mockResolvedValue({ ...base, ...over });
};

describe("Privacy", () => {
  it("says Nothing. in large type and when it was last checked", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-13T07:02:02Z"));
    await open();
    expect(screen.getByText("Nothing.")).toBeInTheDocument();
    expect(screen.getByText("Watching live · checked 4 seconds ago")).toBeInTheDocument();
  });

  it("gives the time of the check when it was long ago, and keeps it out of a live region", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-13T12:00:00Z"));
    await open();
    const line = screen.getByText(/^Watching live · checked/);
    expect(line).toHaveTextContent("Watching live · checked at 07:01");
    expect(line.closest("[aria-live], [role=status], [role=alert]")).toBeNull();
  });

  it("shows an amount, not Nothing., when something was sent", async () => {
    const engine = createMockEngine();
    await view(engine, { sentToInternetBytes: 84e6 });
    await open(engine);
    expect(screen.getByText("84 MB.")).toBeInTheDocument();
    expect(screen.queryByText("Nothing.")).toBeNull();
  });

  it("says plainly when sending cannot be measured, and leans on the ledger", async () => {
    const engine = createMockEngine();
    await view(engine, { sentMeasured: false });
    await open(engine);
    expect(screen.getByText("Not measured.")).toBeInTheDocument();
    expect(screen.queryByText(/Watching live/)).toBeNull();
    expect(screen.getByText(/can’t tell Nocturne what it sends/)).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Downloaded, with your say-so" }),
    ).toBeInTheDocument();
  });

  it("lists what was downloaded with your say-so, and says update checks are off", async () => {
    await open();
    const ledger = screen.getByRole("region", { name: "Downloaded, with your say-so" });
    expect(within(ledger).getByText("Setup: Nocturne’s mind")).toBeInTheDocument();
    expect(within(ledger).getByText("App update 1.0.3")).toBeInTheDocument();
    expect(within(ledger).getByText("Off. Nocturne only checks when you ask.")).toBeInTheDocument();
    expect(within(ledger).getByRole("link", { name: "Change in Settings" })).toHaveAttribute(
      "href",
      "#/settings",
    );
    expect(screen.getByText(/no account, no analytics and no crash reports/)).toBeInTheDocument();
  });

  it("shows what is on this computer and opens that folder", async () => {
    const { platform } = await open();
    const spy = vi.spyOn(platform, "openPath");
    expect(screen.getByText(/4\.3 GB of things you kept,/)).toBeInTheDocument();
    expect(screen.getByText("8.4 GB for Nocturne’s mind")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Space used" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open folder" }));
    expect(spy).toHaveBeenCalledWith("Documents/Nocturne");
  });

  it("exports to the folder the person picks, shows progress and the finished folder", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockResolvedValue("/chosen");
    const open_ = vi.spyOn(platform, "openPath");
    const { engine } = await open(createMockEngine(), platform);
    const spy = vi.spyOn(engine, "exportTo");
    await userEvent.click(screen.getByRole("button", { name: "Export to a folder…" }));
    expect(spy).toHaveBeenCalledWith("/chosen");
    expect(await screen.findByText("Exported to /chosen/Nocturne export.")).toBeInTheDocument();
    expect(screen.getByText(/memories?, \d+ pages? and \d+ originals?\./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open export folder" }));
    expect(open_).toHaveBeenCalledWith("/chosen/Nocturne export");
  });

  it("does nothing when the person cancels the folder choice", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "exportTo");
    await userEvent.click(screen.getByRole("button", { name: "Export to a folder…" }));
    expect(spy).not.toHaveBeenCalled();
  });

  it("shows words and a bar while it runs", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockResolvedValue("/chosen");
    const engine = createMockEngine();
    let finish: (r: Awaited<ReturnType<typeof engine.exportTo>>) => void = () => undefined;
    vi.spyOn(engine, "exportTo").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await open(engine, platform);
    await userEvent.click(screen.getByRole("button", { name: "Export to a folder…" }));
    expect(await screen.findByRole("progressbar", { name: "Exporting" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Export to a folder…" })).toBeDisabled();
    await act(async () => {
      finish({ path: "/chosen/x", memories: 1, pages: 2, originals: 3 });
    });
    expect(await screen.findByText("Exported to /chosen/x.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Export to a folder…" })).toHaveFocus(),
    );
  });

  it("says when the folder picker fails, without an unhandled rejection", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockRejectedValue(new Error("no dialog"));
    await open(createMockEngine(), platform);
    await userEvent.click(screen.getByRole("button", { name: "Export to a folder…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The export didn’t finish.");
  });

  it("shows plain Watching live when it does not know when it last checked", async () => {
    const engine = createMockEngine();
    await view(engine, { checkedAt: null });
    await open(engine);
    expect(screen.getByText("Watching live")).toBeInTheDocument();
    expect(screen.getAllByText("Sent to the internet since setup")).toHaveLength(1);
  });

  it("says when an export fails", async () => {
    const platform = fakePlatform();
    vi.spyOn(platform, "pickFolder").mockResolvedValue("/chosen");
    const engine = createMockEngine();
    vi.spyOn(engine, "exportTo").mockRejectedValue(new Error("no"));
    await open(engine, platform);
    await userEvent.click(screen.getByRole("button", { name: "Export to a folder…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The export didn’t finish.");
  });

  it("locks with the computer's sign-in", async () => {
    const { engine } = await open();
    const spy = vi.spyOn(engine, "updateSettings");
    const toggle = screen.getByRole("switch", { name: "Lock with your computer sign-in" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await userEvent.click(toggle);
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ lock: true, theme: null }));
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });

  it("puts the switch back and says so when locking fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "updateSettings").mockRejectedValue(new Error("no"));
    await open(engine);
    const toggle = screen.getByRole("switch", { name: "Lock with your computer sign-in" });
    await userEvent.click(toggle);
    expect(await screen.findByText("That didn’t save. Nothing was changed.")).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });
});

describe("delete everything", () => {
  const openDialog = async (engine = createMockEngine()) => {
    const shown = await open(engine);
    await userEvent.click(screen.getByRole("button", { name: "Delete everything…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete everything?" });
    return { ...shown, dialog };
  };

  it("needs the phrase, and Enter does nothing until it is typed", async () => {
    const { engine, dialog } = await openDialog();
    const spy = vi.spyOn(engine, "deleteEverything");
    const field = within(dialog).getByRole("textbox", {
      name: "Type “delete everything” to go ahead",
    });
    expect(field).toHaveFocus();
    const confirm = within(dialog).getByRole("button", { name: "Delete everything" });
    expect(confirm).toBeDisabled();
    await userEvent.type(field, "delete{Enter}");
    expect(spy).not.toHaveBeenCalled();
    await userEvent.type(field, " everything");
    expect(confirm).toBeEnabled();
  });

  it("wants the phrase exactly, not in other capitals", async () => {
    const { dialog } = await openDialog();
    const field = within(dialog).getByRole("textbox");
    await userEvent.type(field, "Delete Everything");
    expect(within(dialog).getByRole("button", { name: "Delete everything" })).toBeDisabled();
  });

  it("cannot be cancelled while deleting, and setup is read again afterwards", async () => {
    const engine = createMockEngine();
    let finish: () => void = () => undefined;
    vi.spyOn(engine, "deleteEverything").mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const getSetup = vi.spyOn(engine, "getSetup");
    const { dialog } = await openDialog(engine);
    await userEvent.type(within(dialog).getByRole("textbox"), "delete everything");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete everything" }));
    expect(within(dialog).getByRole("button", { name: "Keep everything" })).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    const before = getSetup.mock.calls.length;
    await act(async () => {
      finish();
    });
    await waitFor(() => expect(window.location.hash).toBe("#/setup?step=welcome"));
    await waitFor(() => expect(getSetup.mock.calls.length).toBeGreaterThan(before));
  });

  it("deletes with the phrase and goes back to setup", async () => {
    const { engine, dialog } = await openDialog();
    const spy = vi.spyOn(engine, "deleteEverything");
    await userEvent.type(within(dialog).getByRole("textbox"), "delete everything");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete everything" }));
    expect(spy).toHaveBeenCalledWith("delete everything");
    await waitFor(() => expect(window.location.hash).toBe("#/setup?step=welcome"));
  });

  it("says so, and stays, when it fails", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "deleteEverything").mockRejectedValue(new Error("no"));
    const { dialog } = await openDialog(engine);
    await userEvent.type(within(dialog).getByRole("textbox"), "delete everything");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete everything" }));
    expect(await within(dialog).findByText(/Nothing was deleted/)).toBeInTheDocument();
    expect(window.location.hash).toBe("#/privacy");
  });

  it("keeps everything on Keep everything or Escape", async () => {
    const { engine, dialog } = await openDialog();
    const spy = vi.spyOn(engine, "deleteEverything");
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep everything" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Delete everything…" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("states", () => {
  it("says so and offers to try again when it cannot be read", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "getPrivacy").mockRejectedValueOnce(new Error("no"));
    renderApp({ hash: "#/privacy", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t open this.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Nothing.")).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
