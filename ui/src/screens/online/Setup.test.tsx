import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CheckOutcome } from "../../engine/bindings";
import type { OnlineScenario } from "../../engine/mock/online";
import { openOnline } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const SETUP = "/settings/online/setup";
const STEP1 = /Have your answers written by a larger mind/;
const STEP2 = "Connect to the service you chose.";
const STEP3 = "Choose which questions may go online.";
const STEP4 = "Set a monthly limit.";
const STEP5 = /Before you turn this on, here is exactly what happens/;

const click = (name: string | RegExp) => userEvent.click(screen.getByRole("button", { name }));
const heading = (name: string | RegExp) => screen.findByRole("heading", { level: 1, name });

async function enter(label: RegExp, text: string) {
  const field = screen.getByLabelText(label);
  await userEvent.clear(field);
  await userEvent.click(field);
  await userEvent.paste(text);
}

async function fill(patch: { url?: string; key?: string; model?: string } = {}) {
  await enter(/^Address/, patch.url ?? "https://api.example.com/v1");
  await enter(/^Key/, patch.key ?? "sk-example-not-a-real-key");
  await enter(/^Which one to use/, patch.model ?? "llama-3.3-70b-instruct");
}

async function toConnect(scenario: OnlineScenario = "off") {
  const opened = await openOnline(SETUP, scenario, STEP1);
  await click("Continue");
  await heading(STEP2);
  return opened;
}

async function toSend(scenario: OnlineScenario = "off") {
  const opened = await toConnect(scenario);
  await fill();
  await click("Check");
  await screen.findByText(/It works\. api\.example\.com answered, using llama-3\.3-70b-instruct\./);
  await click("Continue");
  await heading(STEP3);
  return opened;
}

async function toLimit(scenario: OnlineScenario = "off") {
  const opened = await toSend(scenario);
  await click("Continue");
  await heading(STEP4);
  return opened;
}

describe("setup", () => {
  it("shows the steps in order with a stepper", async () => {
    await openOnline(SETUP, "off", STEP1);
    expect(screen.getByText("Online answers · step 1 of 5")).toBeInTheDocument();
    await click("Continue");
    await heading(STEP2);
    expect(screen.getByText("Online answers · step 2 of 5")).toBeInTheDocument();
  });

  it("explainer: shows exactly what is sent on request", async () => {
    await openOnline(SETUP, "off", STEP1);
    expect(screen.queryByText("when is my eye test?")).toBeNull();
    const toggle = screen.getByRole("button", { name: "What exactly is sent?" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("when is my eye test?")).toBeInTheDocument();
    expect(screen.getByText(/Not sent: the clinic’s letter/)).toBeInTheDocument();
  });

  it("check_required: Continue stays disabled until the check passes, and the key is hidden", async () => {
    const { engine } = await toConnect();
    const key = screen.getByLabelText(/^Key/);
    expect(key).toHaveAttribute("type", "password");
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByText("Continue works once the check passes.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check" })).toBeDisabled();
    await fill();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(key).toHaveAttribute("type", "text");
    await userEvent.click(screen.getByRole("button", { name: "Hide" }));
    expect(key).toHaveAttribute("type", "password");
    const check = vi.spyOn(engine, "checkOnline");
    await click("Check");
    expect(check).toHaveBeenCalledWith({
      baseUrl: "https://api.example.com/v1",
      key: "sk-example-not-a-real-key",
      model: "llama-3.3-70b-instruct",
    });
    await screen.findByText(/It works\./);
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    expect(
      screen.getByText(/call it “your provider \(api\.example\.com\)” on every answer/),
    ).toBeInTheDocument();
  });

  it("check_required: editing a field after it worked asks for a new check", async () => {
    await toConnect();
    await fill();
    await click("Check");
    await screen.findByText(/It works\./);
    await userEvent.type(screen.getByLabelText(/^Which one to use/), "-x");
    expect(screen.queryByText(/It works\./)).toBeNull();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("check_required: a check can be cancelled", async () => {
    const { engine } = await toConnect();
    engine.online.setCheckMs(60_000);
    await fill();
    await click("Check");
    expect(await screen.findByText("Checking with api.example.com…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Checking…" })).toBeDisabled();
    await click("Cancel");
    await waitFor(() => expect(screen.queryByText(/Checking with/)).toBeNull());
    expect(screen.getByRole("button", { name: "Check" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  const OUTCOMES: Array<[CheckOutcome, RegExp, RegExp]> = [
    [
      { type: "unreachable" },
      /Nothing answered at that address\./,
      /including the part after the last slash/,
    ],
    [
      { type: "keyRefused" },
      /api\.example\.com refused the key\./,
      /Some services also need credit/,
    ],
    [
      { type: "noCredit" },
      /api\.example\.com says there’s no credit left\./,
      /Add credit on its website/,
    ],
    [
      { type: "modelMissing" },
      /api\.example\.com doesn’t know a name like “llama-3\.3-70b-instruct”\./,
      /Capitals and dashes matter/,
    ],
    [{ type: "noInternet" }, /This computer isn’t connected to the internet\./, /Nothing was sent/],
    [{ type: "notHttps" }, /That address doesn’t start with https:\/\//, /stays locked on the way/],
    [
      { type: "addressNotAllowed" },
      /points at this computer or your own network/,
      /Nothing was sent/,
    ],
    [{ type: "busy" }, /api\.example\.com is busy right now\./, /Wait a moment/],
    [
      { type: "serverError" },
      /api\.example\.com had a problem on its side\./,
      /Try again in a few minutes/,
    ],
    [
      { type: "emptyReply" },
      /api\.example\.com answered, but with nothing\./,
      /exactly as its website shows it/,
    ],
  ];
  for (const [outcome, title, detail] of OUTCOMES) {
    it(`each_check_outcome_copy: ${outcome.type}`, async () => {
      const { engine } = await toConnect();
      engine.online.forceOutcome(outcome);
      await fill();
      await click("Check");
      expect(await screen.findByText(title)).toBeInTheDocument();
      expect(screen.getByText(detail)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled();
    });
  }

  it("each_check_outcome_copy: marks the field that is wrong", async () => {
    const { engine } = await toConnect();
    engine.online.forceOutcome({ type: "keyRefused" });
    await fill();
    await click("Check");
    await screen.findByText(/refused the key/);
    expect(screen.getByLabelText(/^Key/)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText(/^Address/)).toHaveAttribute("aria-invalid", "false");
  });

  it("phones_row_only_with_away: the phones switch appears only when Away from home is on", async () => {
    await toSend("off");
    expect(screen.queryByRole("switch", { name: "Questions from your phones" })).toBeNull();
  });

  it("phones_row_only_with_away: it appears, on, when Away from home is on", async () => {
    await toSend("offBefore");
    expect(screen.getByRole("switch", { name: "Questions from your phones" })).toBeChecked();
  });

  it("sensitive_off_by_default: health, money and ID stay here until switched on", async () => {
    await toSend();
    expect(screen.getByRole("switch", { name: "Health, money and ID details" })).not.toBeChecked();
    expect(screen.getByText("Private things are never sent")).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /Private things/ })).toBeNull();
  });

  it("limit_200_default: the limit starts at 200 questions", async () => {
    await toLimit();
    expect(screen.getByLabelText(/Stop online answers after/)).toHaveValue("200");
    expect(
      screen.getByText(/Then questions are answered on this computer until the 1st/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Changed from 200/)).toBeNull();
    expect(
      screen.getByText(/doesn’t tell Nocturne what each one costs, so the limit counts questions/),
    ).toBeInTheDocument();
  });

  it("limit_200_default: editing it says it changed", async () => {
    await toLimit();
    const input = screen.getByLabelText(/Stop online answers after/);
    await userEvent.clear(input);
    await userEvent.type(input, "50");
    expect(screen.getByText(/Changed from 200\./)).toBeInTheDocument();
  });

  it("invalid_limit: asks for a whole number and will not go on", async () => {
    await toLimit();
    const input = screen.getByLabelText(/Stop online answers after/);
    for (const bad of ["0", "-3", "2.5", "", "lots"]) {
      await userEvent.clear(input);
      if (bad) await userEvent.type(input, bad);
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Enter a whole number from 1 upwards, like 200.",
      );
      expect(input).toHaveAttribute("aria-invalid", "true");
      expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    }
    await userEvent.clear(input);
    await userEvent.type(input, "7");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
  });

  it("no_limit_confirms: asks first and keeps the limit when told to", async () => {
    await toLimit();
    await userEvent.click(screen.getByRole("button", { name: "Use no limit…" }));
    const dialog = await screen.findByRole("dialog", { name: "Use no limit?" });
    expect(within(dialog).getByText(/your provider charges for each one/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep a limit" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByLabelText(/Stop online answers after/)).toHaveValue("200");
    await userEvent.click(screen.getByRole("button", { name: "Use no limit…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Use no limit" }));
    expect(screen.getByText(/There is no limit\./)).toBeInTheDocument();
    await click("Continue");
    await heading(STEP5);
    expect(screen.getAllByText("No limit").length).toBeGreaterThan(0);
  });

  it("consent_lists_choices: every choice is listed, with a way to change each", async () => {
    await toSend("offBefore");
    await userEvent.click(screen.getByRole("switch", { name: "Health, money and ID details" }));
    await userEvent.click(screen.getByRole("switch", { name: "Questions from your phones" }));
    await click("Continue");
    await heading(STEP4);
    await click("Continue");
    await heading(STEP5);
    const choices = screen.getByRole("heading", { name: "Your choices" })
      .parentElement as HTMLElement;
    const text = choices.textContent ?? "";
    expect(text).toContain("api.example.com");
    expect(text).toContain("llama-3.3-70b-instruct");
    expect(text).toContain("Questions on this computer");
    expect(text).toContain("This computer");
    expect(text).toContain("May be sent");
    expect(text).toContain("Never sent");
    expect(text).toContain("200 questions");
    expect(screen.getByText(/exactly what happens/)).toBeInTheDocument();
    expect(screen.getByText(/can see each question, the lines sent with it/)).toBeInTheDocument();
    expect(screen.getByText(/Nocturne can’t check what the service keeps/)).toBeInTheDocument();
    expect(screen.getByText(/At most 200 questions a month go online/)).toBeInTheDocument();
    await userEvent.click(within(choices).getByRole("button", { name: "Change: Monthly limit" }));
    await heading(STEP4);
  });

  it("turns on at the last step with the choices made, then shows the page on", async () => {
    const { engine } = await toLimit();
    const on = vi.spyOn(engine, "turnOnOnline");
    await click("Continue");
    await heading(STEP5);
    expect(on).not.toHaveBeenCalled();
    await click("Turn on online answers");
    await waitFor(() => expect(on).toHaveBeenCalledTimes(1));
    expect(on).toHaveBeenCalledWith({
      phones: true,
      sendSensitive: false,
      limit: { kind: "questions", questions: 200, usd: null },
    });
    expect(await screen.findByRole("switch", { name: "Online answers are on" })).toBeChecked();
    expect(window.location.hash).toBe("#/settings/online");
    expect(engine.online.abandoned).toBe(0);
  });

  it("a failed turn on says so and offers another go, nothing turned on", async () => {
    const { engine } = await toLimit();
    await click("Continue");
    await heading(STEP5);
    vi.spyOn(engine, "turnOnOnline").mockRejectedValueOnce(new Error("no"));
    await click("Turn on online answers");
    expect(await screen.findByRole("alert")).toHaveTextContent(/nothing was turned on/);
    expect(engine.online.view().state).toEqual({ type: "off" });
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
  });

  it("leaving_abandons: leaving after a check tells the computer to drop it", async () => {
    const { engine, unmount } = await toSend();
    const abandon = vi.spyOn(engine, "abandonOnlineSetup");
    await userEvent.click(screen.getByRole("link", { name: /Online answers/ }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/online"));
    await waitFor(() => expect(abandon).toHaveBeenCalledTimes(1));
    unmount();
  });

  it("leaving_abandons: Not now at the connect step abandons once a check ran", async () => {
    const { engine } = await toConnect();
    const abandon = vi.spyOn(engine, "abandonOnlineSetup");
    await fill();
    await click("Check");
    await screen.findByText(/It works\./);
    await click("Not now");
    await waitFor(() => expect(window.location.hash).toBe("#/settings/online"));
    await waitFor(() => expect(abandon).toHaveBeenCalledTimes(1));
  });

  it("leaving_abandons: leaving before any check has nothing to drop", async () => {
    const { engine } = await openOnline(SETUP, "off", STEP1);
    const abandon = vi.spyOn(engine, "abandonOnlineSetup");
    await click("Not now");
    await waitFor(() => expect(window.location.hash).toBe("#/settings/online"));
    expect(abandon).not.toHaveBeenCalled();
  });

  it("Change… opens the connect step with the address and name filled in, and no key", async () => {
    await openOnline(`${SETUP}?step=connect`, "on", STEP2);
    expect(screen.getByLabelText(/^Address/)).toHaveValue("https://api.example.com/v1");
    expect(screen.getByLabelText(/^Which one to use/)).toHaveValue("llama-3.3-70b-instruct");
    expect(screen.getByLabelText(/^Key/)).toHaveValue("");
  });
});
