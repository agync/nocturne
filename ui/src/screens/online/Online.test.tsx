import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EngineError } from "../../engine/client";
import { renderApp, setViewport } from "../../test/render";
import { onlineEngine, openOnline } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const PAGE = "/settings/online";

describe("online", () => {
  it("off_never_set_up: says it is off, sends nothing and offers the set up", async () => {
    const { engine } = await openOnline(PAGE, "off");
    expect(screen.getByText("Off", { selector: "span" })).toBeInTheDocument();
    expect(
      screen.getByText(/It’s off\. Nothing is sent, and no service is connected/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByText(/your provider/i)).toBeNull();
    const check = vi.spyOn(engine, "checkOnline");
    await userEvent.click(screen.getByRole("button", { name: "Set up…" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/online/setup"));
    expect(check).not.toHaveBeenCalled();
  });

  it("off_never_set_up: names no provider and has no key to forget", async () => {
    await openOnline(PAGE, "off");
    expect(screen.queryByRole("button", { name: /Forget the key/ })).toBeNull();
    expect(screen.getByText("Always stays here")).toBeInTheDocument();
  });

  it("checking: has its own view, with no switch and nothing to turn on", async () => {
    await openOnline(PAGE, "checking");
    expect(screen.getByText("Checking", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText(/Nothing is turned on yet/)).toBeInTheDocument();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("button", { name: /Set up|Turn on/ })).toBeNull();
  });

  it("warnings keep their buttons outside the status text", async () => {
    await openOnline(PAGE, "refused");
    const button = screen.getByRole("button", { name: "Check the key…" });
    expect(button.closest("[role=status]")).toBeNull();
    expect(
      screen.getByText("Your provider refused the key").closest("[role=status]"),
    ).not.toBeNull();
  });

  it("off_before: turning on again takes one switch and sends no form", async () => {
    const { engine } = await openOnline(PAGE, "offBefore");
    expect(screen.getByText("Your provider (api.example.com)")).toBeInTheDocument();
    const on = vi.spyOn(engine, "turnOnOnline");
    const check = vi.spyOn(engine, "checkOnline");
    await userEvent.click(screen.getByRole("switch", { name: "Online answers are off" }));
    await waitFor(() => expect(on).toHaveBeenCalledTimes(1));
    expect(check).not.toHaveBeenCalled();
    expect(await screen.findByRole("switch", { name: "Online answers are on" })).toBeChecked();
  });

  it("on_counts_questions: shows the count against the limit", async () => {
    await openOnline(PAGE, "on");
    const meter = screen.getByRole("img", { name: "38 of 200 questions this month" });
    expect(meter).toBeInTheDocument();
    expect(screen.getByText("38")).toBeInTheDocument();
    expect(screen.getByText("questions sent online")).toBeInTheDocument();
    expect(screen.getByText(/Of your limit of 200\. Resets on 1 /)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Online answers are on" })).toBeChecked();
    expect(screen.getByText("llama-3.3-70b-instruct", { exact: false })).toBeInTheDocument();
  });

  it("cost_only_when_reported: says Not told until the provider reports cost", async () => {
    const { unmount } = await openOnline(PAGE, "on");
    expect(screen.getByText("Not told")).toBeInTheDocument();
    expect(screen.queryByText(/\$/)).toBeNull();
    unmount();
    await openOnline(PAGE, "cost");
    expect(screen.getByText("$0.42")).toBeInTheDocument();
    expect(screen.queryByText("Not told")).toBeNull();
    expect(
      screen.getByText(/Nocturne doesn’t ask your provider how much credit is left/),
    ).toBeInTheDocument();
  });

  it("cost_only_when_reported: money limits are offered only with cost", async () => {
    const { unmount } = await openOnline(PAGE, "on");
    const menu = screen.getByRole("combobox", { name: "Monthly limit" });
    expect(within(menu).queryByRole("option", { name: /\$/ })).toBeNull();
    unmount();
    await openOnline(PAGE, "cost");
    expect(
      within(screen.getByRole("combobox", { name: "Monthly limit" })).getByRole("option", {
        name: "$5.00 a month",
      }),
    ).toBeInTheDocument();
  });

  it("paused_offers_change_limit: a limit pause points at the limit and a higher one lifts it", async () => {
    const { engine } = await openOnline(PAGE, "limit");
    expect(screen.getByText("On · answering here")).toBeInTheDocument();
    expect(screen.getByText("Paused by your limit")).toBeInTheDocument();
    expect(screen.getByText(/You’ve asked 200 online questions this month/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change limit" }));
    const menu = screen.getByRole("combobox", { name: "Monthly limit" });
    expect(menu).toHaveFocus();
    await userEvent.selectOptions(menu, "500 questions");
    await waitFor(() => expect(engine.online.view().state).toEqual({ type: "on" }));
    expect(await screen.findByRole("switch", { name: "Online answers are on" })).toBeChecked();
  });

  it("refused_offers_check_the_key: goes to the connect step, with no key shown", async () => {
    await openOnline(PAGE, "refused");
    expect(screen.getByText("Your provider refused the key")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Check the key…" }));
    await waitFor(() =>
      expect(window.location.hash).toBe("#/settings/advanced/online?from=online"),
    );
    const key = await screen.findByLabelText(/^Key/);
    expect(key).toHaveValue("");
    expect(key).toHaveAttribute("type", "password");
  });

  it("refused_offers_check_the_key: no credit offers to check again, a problem to try again", async () => {
    const { unmount } = await openOnline(PAGE, "noCredit");
    expect(screen.getByText("Your provider says there’s no credit left")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check again" })).toBeInTheDocument();
    unmount();
    const { engine } = await openOnline(PAGE, "problem");
    expect(screen.getByText("Online answers aren’t working right now")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(engine.online.view().state).toEqual({ type: "on" }));
  });

  it("refused_offers_check_the_key: an unreadable key asks to check it", async () => {
    await openOnline(PAGE, "keyUnavailable");
    expect(screen.getByText("Nocturne can’t read the saved key")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check the key…" })).toBeInTheDocument();
  });

  it("turn_off_no_confirm: the switch turns it off at once, and Undo turns it back on", async () => {
    const { engine } = await openOnline(PAGE, "on");
    const off = vi.spyOn(engine, "turnOffOnline");
    await userEvent.click(screen.getByRole("switch", { name: "Online answers are on" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(off).toHaveBeenCalledWith(false));
    expect(
      await screen.findByText(/Online answers are off\. Questions will be answered/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(engine.online.view().state).toEqual({ type: "on" }));
  });

  it("forget_confirms_and_names_host: asks first, names the service, then forgets", async () => {
    const { engine } = await openOnline(PAGE, "on");
    const off = vi.spyOn(engine, "turnOffOnline");
    await userEvent.click(screen.getByRole("button", { name: "Turn off and forget the key…" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn off and forget the key?" });
    expect(off).not.toHaveBeenCalled();
    expect(
      within(dialog).getByText(/key for your provider \(api\.example\.com\)/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/stays listed for 90 days/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Turn off and forget" }));
    await waitFor(() => expect(off).toHaveBeenCalledWith(true));
    expect(
      await screen.findByText("Turned off, and the key is forgotten on this computer."),
    ).toBeInTheDocument();
    await waitFor(() => expect(engine.online.view().setUp).toBe(false));
  });

  it("forget_confirms_and_names_host: keeping it on changes nothing", async () => {
    const { engine } = await openOnline(PAGE, "on");
    const off = vi.spyOn(engine, "turnOffOnline");
    await userEvent.click(screen.getByRole("button", { name: "Turn off and forget the key…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Keep it on" }));
    expect(off).not.toHaveBeenCalled();
    expect(engine.online.view().state).toEqual({ type: "on" });
  });

  it("phones_row_only_with_away: the phones row follows whether Away from home is on", async () => {
    const engine = (await openOnline(PAGE, "on")).engine;
    expect(screen.getByRole("switch", { name: "Questions from your phones" })).toBeChecked();
    expect(engine.online.view().phonesAvailable).toBe(true);
  });

  it("limit: no limit is chosen at once, and a limit can be set again", async () => {
    const { engine } = await openOnline(PAGE, "on");
    const update = vi.spyOn(engine, "updateOnlineSettings");
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Monthly limit" }),
      "No limit",
    );
    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(engine.online.view().limit.kind).toBe("none");
    expect(await screen.findByText(/No limit is set\. Resets on/)).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Monthly limit" }),
      "200 questions",
    );
    await waitFor(() => expect(engine.online.view().limit.kind).toBe("questions"));
  });
});

describe("online, when the computer cannot be read", () => {
  it("online.unavailable: says Online answers is unavailable, and Advanced says why", async () => {
    setViewport(1280);
    const engine = onlineEngine();
    engine.getOnline = async () => {
      throw new EngineError("unavailable", "this is not available in a browser");
    };
    renderApp({ hash: "#/settings/online", engine });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Online answers aren’t available here.");
    expect(alert).not.toHaveTextContent("What went wrong");
  });

  it("online.unavailable: the Advanced page shows the real error", async () => {
    setViewport(1280);
    const engine = onlineEngine();
    engine.getOnline = async () => {
      throw new EngineError("internal", "the store is locked");
    };
    renderApp({ hash: "#/settings/advanced/online", engine });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t open this.");
    expect(alert).toHaveTextContent("What went wrong: the store is locked");
  });
});

describe("the journal on the Online answers page", () => {
  it("has the Your journal row and both Always stays here additions", async () => {
    await openOnline(PAGE, "on");
    const row = screen.getByText("Your journal").closest("li") as HTMLElement;
    expect(within(row).getByText("Entries and good things, when they help")).toBeInTheDocument();
    expect(
      within(row).getByText(
        "A line from an entry or a good thing may go with a question it helps answer. They can’t be marked private. Check-ins, reflections and recaps never go.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Your check-ins, reflections and recaps")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Journal entries and good things can’t be marked private, so while this is on, a line from one can go with a question it helps answer.",
      ),
    ).toBeInTheDocument();
  });
});

describe("only what is marked private is held back", () => {
  it("has no health, money or ID switch, and says to mark things private", async () => {
    await openOnline(PAGE, "on");
    expect(screen.queryByRole("switch", { name: /Health, money and ID/ })).toBeNull();
    expect(screen.queryByText(/health, money/i)).toBeNull();
    expect(
      screen.getByText(
        "Nocturne doesn’t guess what is private. Mark anything you never want sent as private.",
      ),
    ).toBeInTheDocument();
  });
});
