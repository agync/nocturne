import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { QrMatrix } from "./QrMatrix";
import { manualEngine, openAway } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const ADD = "/settings/away/add";

describe("add", () => {
  it("code_counts_down_and_expires: counts in seconds, then says it is used up", {
    timeout: 15_000,
  }, async () => {
    const engine = manualEngine("onAlone");
    const real = engine.beginPairing.bind(engine);
    vi.spyOn(engine, "beginPairing").mockImplementation(async () => ({
      ...(await real()),
      expiresAt: new Date(Date.now() + 5000).toISOString(),
    }));
    await openAway(ADD, "onAlone", "Add a phone", engine);
    const timer = await screen.findByRole("timer");
    expect(timer).toHaveTextContent(/^0:0\d left$/);
    expect(
      await screen.findByText("This code has expired.", {}, { timeout: 9000 }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Make a new code" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Code to scan/ })).toBeNull();
  });

  it("code_counts_down_and_expires: says how many minutes are left, in words", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    expect(await screen.findByRole("timer")).toHaveTextContent(/^[45]:\d\d left$/);
    const live = screen.getByText(/minutes left on this code\./);
    expect(live).toHaveClass("sr-only");
    expect(live.closest("[role=status]")).not.toBeNull();
    expect((await engine.getRemote()).pairing?.phase.type).toBe("waitingForScan");
  });

  it("typed_alternative_shown: gives the address and the code to type", async () => {
    await openAway(ADD, "onAlone", "Add a phone");
    expect(await screen.findByText("Can’t scan?")).toBeInTheDocument();
    expect(screen.getByText("amber-otter-lamp-quiet.trycloudflare.com/p")).toBeInTheDocument();
    expect(screen.getByText(/and type this code:/)).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "Code to scan with your phone’s camera" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Check that three words match on both screens/)).toBeInTheDocument();
  });

  it("typed_code_in_four_groups: shows four groups of six and copies the whole code", async () => {
    await openAway(ADD, "onAlone", "Add a phone");
    const groups = within(
      await screen.findByRole("list", { name: "Code to type, in four groups of six" }),
    );
    const items = groups.getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual(["7KQ2M9", "XDR4TB", "0HJ5WC", "P8N3VA"]);
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    await userEvent.click(screen.getByRole("button", { name: "Copy code" }));
    expect(writeText).toHaveBeenCalledWith("7KQ2M9XDR4TB0HJ5WCP8N3VA");
  });

  it("paused_after_wrong_codes: says adding phones is paused and until when", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    await screen.findByRole("timer");
    act(() => engine.remote.pauseForWrongCodes());
    expect(await screen.findByText("Adding phones is paused.")).toBeInTheDocument();
    expect(screen.getByText(/starts again at \d\d:\d\d/)).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("not reachable: says it has to be on first", async () => {
    await openAway(ADD, "offBefore", "Add a phone");
    expect(
      await screen.findByText("Away from home isn’t ready for a new phone."),
    ).toBeInTheDocument();
  });

  it("dialog_focus_and_words_read_out: takes focus and reads the three words one by one", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    const dialog = await screen.findByRole("alertdialog", { name: "A phone wants to connect" });
    await waitFor(() => expect(dialog).toHaveFocus());
    expect(dialog).toHaveAccessibleDescription(
      /Pixel\. Check your phone shows the same three words\./,
    );
    const words = within(within(dialog).getByRole("list", { name: "The three words to check" }));
    expect(words.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "harbour",
      "maple",
      "lantern",
    ]);
    expect(within(dialog).getByText(/Refused by itself in \d:\d\d/)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/it isn’t a phone you’re holding, don’t allow it/),
    ).toBeInTheDocument();
    // Tab stays inside, and nothing behind it can be reached.
    await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(dialog).toHaveAttribute("aria-modal", "true");
  });

  it("allow_and_refuse_call_decide: Don't allow refuses and leaves no phone", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    const decide = vi.spyOn(engine, "decidePairing");
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    const dialog = await screen.findByRole("alertdialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Don’t allow" }));
    const id = (await engine.getRemote()).pairing?.pairingId;
    expect(decide).toHaveBeenCalledWith(id, false);
    expect(await screen.findByText("That phone wasn’t added.")).toBeInTheDocument();
    expect((await engine.getRemote()).devices).toEqual([]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("allow_and_refuse_call_decide: Allow adds the phone and says so", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    const decide = vi.spyOn(engine, "decidePairing");
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    const dialog = await screen.findByRole("alertdialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Allow" }));
    const id = (await engine.getRemote()).pairing?.pairingId;
    expect(decide).toHaveBeenCalledWith(id, true);
    expect(await screen.findByText("Pixel is added.")).toBeInTheDocument();
    expect((await engine.getRemote()).devices.map((d) => d.name)).toEqual(["Pixel"]);
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/away"));
  });

  it("Escape in the dialog means Don't allow", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    const decide = vi.spyOn(engine, "decidePairing");
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    await screen.findByRole("alertdialog");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(decide).toHaveBeenCalledWith(expect.any(String), false));
  });

  it("leaving the first phone's page unadded turns the connection off again", async () => {
    const { engine } = await openAway(`${ADD}?setup=1`, "onAlone", "Add a phone");
    await screen.findByRole("timer");
    const stop = vi.spyOn(engine, "stopRemote");
    act(() => {
      window.location.hash = "#/privacy";
    });
    await waitFor(() => expect(stop).toHaveBeenCalledWith(false));
  });
});

describe("add, in flight", () => {
  it("escape_after_allow: Escape while an Allow is on its way changes nothing", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    let release: () => void = () => undefined;
    const real = engine.decidePairing.bind(engine);
    const decide = vi.spyOn(engine, "decidePairing").mockImplementation(
      (id, allow) =>
        new Promise((resolve) => {
          release = () => resolve(real(id, allow));
        }),
    );
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    const dialog = await screen.findByRole("alertdialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Allow" }));
    await userEvent.keyboard("{Escape}");
    await userEvent.dblClick(within(dialog).getByRole("button", { name: "Allow" }));
    expect(decide).toHaveBeenCalledTimes(1);
    expect(decide).toHaveBeenCalledWith(expect.any(String), true);
    await act(async () => release());
    expect(await screen.findByText("Pixel is added.")).toBeInTheDocument();
  });

  it("moves focus to the result once the phone is added or refused", async () => {
    const { engine } = await openAway(ADD, "onAlone", "Add a phone");
    await screen.findByRole("timer");
    act(() => engine.remote.phoneArrives());
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Don’t allow" }),
    );
    const heading = await screen.findByRole("heading", { name: "That phone wasn’t added." });
    await waitFor(() => expect(heading).toHaveFocus());
  });

  it("says that leaving turns it off again, during the first setup only", async () => {
    await openAway(`${ADD}?setup=1`, "onAlone", "Add a phone");
    expect(
      await screen.findByText(/Leaving at any step turns Away from home back off/),
    ).toBeInTheDocument();
  });
});

describe("qr", () => {
  it("matrix_renders_size: draws the modules the computer made, with a quiet border", () => {
    const modules = ["10101", "01010", "10101", "01010", "10101"].join("");
    const { container } = render(<QrMatrix qr={{ size: 5, modules }} label="A code" />);
    const svg = screen.getByRole("img", { name: "A code" });
    expect(svg).toHaveAttribute("viewBox", "0 0 13 13");
    expect(svg).toHaveAttribute("data-size", "5");
    const path = container.querySelector("path")?.getAttribute("d") ?? "";
    expect(path.match(/M/g)).toHaveLength(13);
    expect(path).toContain("M4 4h1v1h-1z");
  });
});
