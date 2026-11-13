import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openAway } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const PHONES = "/settings/away/phones";
const ACTIVITY = "/settings/away/activity";

describe("phones", () => {
  it("lists each phone with how it was last used and when it was paired", async () => {
    await openAway(PHONES, "inUse", "Phones");
    const list = screen.getByRole("list", { name: "Your phones" });
    expect(within(list).getByText("Pixel")).toBeInTheDocument();
    expect(within(list).getByText("Using Nocturne now")).toBeInTheDocument();
    expect(within(list).getByText("Last used yesterday, 18:40")).toBeInTheDocument();
    expect(within(list).getAllByText(/^Paired /)).toHaveLength(2);
  });

  it("rename_inline_saves: renames in place, by keyboard, and puts focus back", async () => {
    const { engine } = await openAway(PHONES, "on", "Phones");
    await userEvent.click(screen.getByRole("button", { name: "Rename Work phone" }));
    const field = screen.getByRole("textbox", { name: "New name for Work phone" });
    expect(field).toHaveFocus();
    await userEvent.clear(field);
    await userEvent.type(field, "Spare{Enter}");
    await waitFor(async () =>
      expect((await engine.getRemote()).devices.map((d) => d.name)).toContain("Spare"),
    );
    expect(await screen.findByRole("button", { name: "Rename Spare" })).toHaveFocus();
    expect(screen.queryByRole("textbox", { name: /New name for/ })).toBeNull();
  });

  it("rename_inline_saves: refuses an empty name and cancels with Escape", async () => {
    const { engine } = await openAway(PHONES, "on", "Phones");
    await userEvent.click(screen.getByRole("button", { name: "Rename Pixel" }));
    const field = screen.getByRole("textbox", { name: "New name for Pixel" });
    await userEvent.clear(field);
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByText("Give the phone a name.")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: /New name for/ })).toBeNull();
    expect((await engine.getRemote()).devices.map((d) => d.name)).toEqual(["Pixel", "Work phone"]);
  });

  it("remove_confirms_then_calls: asks, says nothing is deleted, then removes", async () => {
    const { engine } = await openAway(PHONES, "on", "Phones");
    const remove = vi.spyOn(engine, "removeDevice");
    await userEvent.click(screen.getByRole("button", { name: "Remove Work phone" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove Work phone?" });
    expect(within(dialog).getByRole("button", { name: "Remove" })).toHaveAttribute(
      "data-variant",
      "forget",
    );
    expect(
      within(dialog).getByText("It stops working at once. You can pair it again later."),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/Nothing on this computer is deleted/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
    expect(remove).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Remove Work phone" }));
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove" }),
    );
    await waitFor(() => expect(remove).toHaveBeenCalledWith("work"));
    await waitFor(() => expect(screen.queryByText("Work phone")).toBeNull());
  });

  it("empty_state: invites adding a first phone", async () => {
    await openAway(PHONES, "onAlone", "Phones");
    expect(
      screen.getByText(/No phones yet\. Add one to use Nocturne when you’re out\./),
    ).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Your phones" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Add a phone" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away/add"));
  });

  it("says a phone is waiting for the new address", async () => {
    await openAway(PHONES, "relink", "Phones");
    expect(screen.getByText("Waiting for the new address")).toBeInTheDocument();
  });
});

describe("activity", () => {
  it("groups_by_day: lists what happened under Today and Yesterday", async () => {
    await openAway(ACTIVITY, "on", "Recent activity");
    expect(await screen.findByRole("heading", { name: "Today" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Yesterday" })).toBeInTheDocument();
    expect(screen.getByText("Pixel kept a photo")).toBeInTheDocument();
    expect(screen.getByText("Pixel asked a question")).toBeInTheDocument();
    expect(screen.getByText("You allowed Pixel")).toBeInTheDocument();
    expect(screen.getByText("Work phone kept a voice note")).toBeInTheDocument();
    expect(screen.getByText("Work phone asked 2 questions")).toBeInTheDocument();
    expect(screen.getByText("Work phone took the new address")).toBeInTheDocument();
    const today = screen.getByRole("heading", { name: "Today" }).closest("section");
    expect(today).not.toBeNull();
    expect(within(today as HTMLElement).queryByText(/voice note/)).toBeNull();
  });

  it("unusual_banner: says who was refused and offers to turn it off", async () => {
    const { engine } = await openAway(ACTIVITY, "on", "Recent activity");
    expect(
      await screen.findByText(/Someone tried 5 wrong pairing codes at 03:41 today\./),
    ).toBeInTheDocument();
    expect(screen.getByText(/Nobody got in/)).toBeInTheDocument();
    expect(screen.queryByText(/paused|15 minutes/i)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(screen.queryByText(/Nobody got in/)).toBeNull();
    expect((await engine.getRemote()).state.type).toBe("on");
  });

  it("unusual_banner: Turn off for now turns it off", async () => {
    const { engine } = await openAway(ACTIVITY, "on", "Recent activity");
    await userEvent.click(await screen.findByRole("button", { name: "Turn off for now" }));
    await waitFor(async () => expect((await engine.getRemote()).state.type).toBe("off"));
  });

  it("never_shows_content: lists only what happened, with no words from what was asked or kept", async () => {
    const { engine } = await openAway(ACTIVITY, "on", "Recent activity");
    await screen.findByRole("heading", { name: "Today" });
    const text = document.querySelector("main")?.textContent ?? "";
    expect(text).not.toMatch(/Leo|school|doctor|letter/);
    expect(screen.getByText("Never what was asked or kept.", { exact: false })).toBeInTheDocument();
    for (const event of await engine.listRemoteEvents(50)) {
      expect(Object.keys(event).sort()).toEqual(["action", "at", "count", "deviceName", "result"]);
    }
  });

  it("shows an empty list in words", async () => {
    await openAway(ACTIVITY, "onAlone", "Recent activity");
    expect(
      await screen.findByText(/Nothing yet\. What your phones do is listed here\./),
    ).toBeInTheDocument();
  });
});
