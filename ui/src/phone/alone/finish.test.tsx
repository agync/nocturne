import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { scriptedNoteRows } from "./testing/homeMock";

afterEach(async () => {
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
  scriptedNoteRows.clear();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

const thisPhone = async () =>
  fireEvent.click(await screen.findByRole("button", { name: "This phone" }));

describe("saving a backup through the phone's sheet", () => {
  it("changes nothing when the sheet is closed", async () => {
    const services = await app("m=kept");
    const save = vi.spyOn(services.home, "saveBackupFile");
    await thisPhone();
    fireEvent.click(await screen.findByRole("button", { name: /Save a backup…/ }));
    for (const label of ["Choose a passphrase", "Type it again"])
      fireEvent.change(await screen.findByLabelText(label), {
        target: { value: "closes the sheet" },
      });
    fireEvent.click(screen.getByLabelText("I’ve written it down or saved it somewhere safe"));
    fireEvent.click(screen.getByRole("button", { name: "Save the backup…" }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith("closes the sheet", expect.any(Function)),
    );
    expect(await screen.findByRole("button", { name: "Save the backup…" })).toBeVisible();
    expect(screen.queryByText("Your backup is saved.")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      await screen.findByRole("button", { name: /Save a backup….*No backup yet/ }),
    ).toBeVisible();
  }, 15_000);
});

describe("bringing back a backup file", () => {
  it("shows the file's size, made {day} only when known, and lets go of it on leaving", async () => {
    const services = await app("m=first&s=welcome");
    const letGo = vi.spyOn(services.home, "letGoBackupFile");
    const choose = vi
      .spyOn(services.home, "chooseBackupFile")
      .mockResolvedValueOnce({ path: "/cache/a", name: "a.nocturne", bytes: 2_000_000 });
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    expect(await screen.findByText("a.nocturne")).toBeInTheDocument();
    expect(screen.getByText("2 MB")).toBeInTheDocument();
    expect(screen.queryByText(/made/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(await screen.findByText(/^412 MB · made /)).toBeInTheDocument();
    expect(choose).toHaveBeenCalledTimes(2);
    const [back] = screen.getAllByRole("button", { name: "Back" });
    if (back) fireEvent.click(back);
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeVisible();
    await waitFor(() => expect(letGo).toHaveBeenCalled());
  }, 15_000);

  it("says a file that is not a backup can't be restored", async () => {
    const services = await app("m=first&s=welcome");
    const { HomeRequestError } = await import("./api");
    vi.spyOn(services.home, "chooseBackupFile").mockRejectedValueOnce(
      new HomeRequestError("notABackup"),
    );
    fireEvent.click(await screen.findByRole("button", { name: "I have a backup" }));
    fireEvent.click(await screen.findByRole("button", { name: "Choose the file" }));
    expect(await screen.findByText("This file can’t be restored")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose the file" })).toBeVisible();
  }, 15_000);
});

describe("what was put aside", () => {
  it("lists it in This phone, then brings it back or deletes it", async () => {
    await app("m=aside");
    await thisPhone();
    const memory = await screen.findByRole("region", { name: "Your memory" });
    const row = await within(memory).findByRole("button", { name: /^Put aside .*20 November…$/ });
    fireEvent.click(row);
    expect(await screen.findByRole("heading", { name: /^Put aside .*20 November$/ })).toBeVisible();
    expect(screen.getByText("388 MB")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Delete it…" }));
    expect(
      await screen.findByRole("heading", { name: "Delete what was put aside?" }),
    ).toBeVisible();
    expect(
      screen.getByText(
        /^117 things kept, 41 journal entries and 36 photos and recordings from before .*\. This can’t be undone\.$/,
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep it" }));

    fireEvent.click(await screen.findByRole("button", { name: "Bring it back…" }));
    expect(
      await screen.findByRole("heading", { name: "Go back to what was here before?" }),
    ).toBeVisible();
    expect(
      screen.getByText(
        "What’s on this phone now is put aside instead, so you can still change your mind.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Bring it back" }));
    const back = await screen.findByRole("region", { name: "Your memory" });
    expect(
      await within(back).findByRole("button", { name: /^Put aside .*21 November…$/ }),
    ).toBeVisible();

    fireEvent.click(within(back).getByRole("button", { name: /^Put aside/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete it…" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete it" }));
    const after = await screen.findByRole("region", { name: "Your memory" });
    await waitFor(() =>
      expect(within(after).queryByRole("button", { name: /^Put aside/ })).toBeNull(),
    );
  }, 15_000);
});

describe("deleting what the calendar brought", () => {
  it("previews it from This phone, then deletes it", async () => {
    const services = await app("m=kept");
    const remove = vi.spyOn(services.home, "deleteSource");
    await thisPhone();
    fireEvent.click(await screen.findByRole("button", { name: "Delete what was kept…" }));
    expect(
      await screen.findByRole("heading", { name: "Delete the calendar kept in Nocturne?" }),
    ).toBeVisible();
    expect(screen.getByText("It goes from this phone for good.")).toBeInTheDocument();
    expect(
      await screen.findByText("6 appointments from your calendar, and their earlier versions"),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Also stop showing your calendar in Nocturne/)).toBeChecked();
    fireEvent.click(screen.getByLabelText("Health"));
    expect(
      await screen.findByRole("heading", { name: "Delete the health readings kept in Nocturne?" }),
    ).toBeVisible();
    expect(await screen.findByText("There is nothing left to delete here.")).toBeInTheDocument();
    expect(screen.queryByLabelText(/Also stop/)).toBeNull();
    fireEvent.click(screen.getByLabelText("Your calendar"));
    await screen.findByText(/6 appointments/);
    fireEvent.click(screen.getByLabelText(/Also stop showing/));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith("kept-token", "calendar", false));
    expect(await screen.findByRole("heading", { name: "Deleted." })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(await screen.findByRole("region", { name: "Your memory" })).toBeInTheDocument();
    expect(screen.queryByText(/computer/)).toBeNull();
  }, 15_000);
});

describe("a phone that is nearly full", () => {
  it("says so on Today, on Keep and in This phone", async () => {
    const services = await app("m=full");
    const free = vi.spyOn(services.home, "freeSpace");
    const banner = await screen.findByText("This phone is nearly full.");
    expect(banner.closest("[role=status]")).toHaveTextContent(
      "This phone is nearly full. Free some room so Nocturne can keep more.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByText("This phone is nearly full.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Back to home" }));
    await thisPhone();
    const memory = await screen.findByRole("region", { name: "Your memory" });
    expect(
      within(memory).getByText(
        "This phone is nearly full. Free some room so Nocturne can keep more.",
      ),
    ).toBeInTheDocument();
    // Asked when the app opened, then only when it comes back to the front.
    const opened = free.mock.calls.length;
    expect(opened).toBeLessThanOrEqual(1);
    document.dispatchEvent(new Event("visibilitychange"));
    await waitFor(() => expect(free).toHaveBeenCalledTimes(opened + 1));
  }, 15_000);

  it("shows no banner with room to spare", async () => {
    await app("m=kept");
    await screen.findByRole("heading", { name: "Kept today · 2" });
    expect(screen.queryByText("This phone is nearly full.")).toBeNull();
  }, 15_000);
});

describe("forgetting and deleting from a thing's page", () => {
  it("has nothing to forget with no mind, and deletes the thing", async () => {
    await app("m=kept");
    fireEvent.click(await screen.findByRole("button", { name: /Ask about the swap/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Forget…" }));
    expect(
      await screen.findByRole("heading", { name: "There is nothing left to delete here." }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Keep it" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete…" }));
    expect(await screen.findByRole("heading", { name: "Delete 1 thing?" })).toBeVisible();
    expect(
      screen.getByText(
        "This will be deleted from this phone for good, along with anything Nocturne worked out from it.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete 1 thing" }));
    expect(await screen.findByRole("heading", { name: "Deleted." })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Ask about the swap/ })).toBeNull(),
    );
    expect(await screen.findByRole("heading", { name: "Kept today · 1" })).toBeInTheDocument();
  }, 15_000);
});
