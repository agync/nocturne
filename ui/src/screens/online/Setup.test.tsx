import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CheckOutcome } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import type { OnlineScenario } from "../../engine/mock/online";
import { chipsIn, sentence } from "../../test/sentence";
import { openOnline } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const SETUP = "/settings/online/setup";
const STEP1 = /Have your answers written by an online service you choose/;
const STEP2 = "Choose the online service.";
const CONNECT = "Connect to the service you chose.";
const ADVANCED = "/settings/advanced/online?from=online";
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
  return openOnline(ADVANCED, scenario, CONNECT);
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

  it("service_step: the plain steps send the person to Advanced details for the form", async () => {
    await openOnline(SETUP, "off", STEP1);
    await click("Continue");
    await heading(STEP2);
    expect(screen.getByText(/added under Advanced details/)).toBeInTheDocument();
    expect(screen.queryByLabelText(/^Address/)).toBeNull();
    expect(screen.queryByLabelText(/^Key/)).toBeNull();
    expect(screen.queryByText(/API/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Continue" })).toBeNull();
    await click("Open Advanced details");
    await heading(CONNECT);
    expect(window.location.hash).toBe("#/settings/advanced/online?from=online");
  });

  it("service_step: says where the form is later, with the words of the controls on the way", async () => {
    await openOnline(`${SETUP}?step=send`, "off", STEP2);
    const where = screen.getByText(
      sentence(
        "Later, find it in Settings, then Show advanced details, then Connect an online service.",
      ),
    );
    expect(chipsIn(where)).toEqual([
      "Settings",
      "Show advanced details",
      "Connect an online service",
    ]);
  });

  it("explainer: gives three short lines and keeps the rest behind one expander", async () => {
    await openOnline(SETUP, "off", STEP1);
    expect(
      screen.getByText(
        "Your question, with a few short lines from what you kept that help answer it.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Your photos, recordings and documents themselves, and anything you marked private.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "An account with that service. Most charge a little for each question. Setting up means copying three details from its website.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Nocturne still finds what you kept/)).toBeNull();
    expect(screen.queryByText(/Everything else stays here/)).toBeNull();
    expect(screen.queryByText("Stays on this computer")).toBeNull();
    expect(screen.queryByText(/Most services charge a little/)).toBeNull();
    expect(screen.queryByRole("button", { name: "What exactly is sent?" })).toBeNull();
  });

  it("explainer: the expander holds every fact, the example and the side column", async () => {
    await openOnline(SETUP, "off", STEP1);
    expect(screen.queryByText("when is my eye test?")).toBeNull();
    const toggle = screen.getByRole("button", {
      name: "Everything that is sent, and what stays here",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).not.toHaveAttribute("aria-controls");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const panel = document.getElementById(toggle.getAttribute("aria-controls") ?? "");
    expect(panel).not.toBeNull();
    const inside = within(panel as HTMLElement);
    expect(inside.getByText("when is my eye test?")).toBeInTheDocument();
    expect(inside.getByText(/Not sent: the clinic’s letter/)).toBeInTheDocument();
    const labels = Array.from((panel as HTMLElement).querySelectorAll("dt")).map(
      (dt) => dt.textContent,
    );
    expect(labels).toEqual([
      "What is sent",
      "Never sent",
      "Who sees it",
      "What changes",
      "What it costs",
      "What you need",
    ]);
    expect(
      inside.getByText(/Nocturne also keeps back lines that look like health, money or ID details/),
    ).toBeInTheDocument();
    expect(
      inside.getByText(
        "An account with an online service. You add its details in step 2, and Nocturne checks that they work.",
      ),
    ).toBeInTheDocument();
    expect(inside.getByText("Stays on this computer")).toBeInTheDocument();
    expect(inside.getByText("Checking where each answer came from")).toBeInTheDocument();
    expect(
      inside.getByText("If the service can’t be reached, this computer answers, and says so."),
    ).toBeInTheDocument();
    const off = inside.getByText(sentence("Turn it off at any time in Settings or Privacy."));
    expect(chipsIn(off)).toEqual(["Settings", "Privacy"]);
  });

  it("explainer: the bar holds Continue first, Not now after, and the step's line", async () => {
    await openOnline(SETUP, "off", STEP1);
    const bar = document.querySelector<HTMLElement>("[data-action-bar]");
    expect(bar).not.toBeNull();
    const buttons = within(bar as HTMLElement).getAllByRole("button");
    expect(buttons.map((b) => b.textContent)).toEqual(["Continue", "Not now"]);
    expect(
      within(bar as HTMLElement).getByText("Nothing is turned on until the last step."),
    ).toBeVisible();
  });

  it("service_step: without a passed check, going straight to a later step asks for one", async () => {
    await openOnline(`${SETUP}?step=send`, "off", STEP2);
    expect(screen.getByRole("button", { name: "Open Advanced details" })).toBeInTheDocument();
  });

  it("service_step: a passed check in Advanced details carries on at what to send, without dropping it", async () => {
    const { engine } = await toConnect();
    const abandon = vi.spyOn(engine, "abandonOnlineSetup");
    await fill();
    await click("Check");
    await screen.findByText(/It works\./);
    await click("Continue");
    await heading(STEP3);
    expect(window.location.hash).toBe("#/settings/online/setup?step=send");
    expect(abandon).not.toHaveBeenCalled();
    await click("Back");
    await heading(STEP2);
    expect(
      screen.getByText(/Connected to api\.example\.com\. To use a different service/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
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
      /That address can’t be used\./,
      /Use the address your provider gives, with nothing after it/,
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

  it("each_check_outcome_copy: a refused address never claims to be a local one", async () => {
    const { engine } = await toConnect();
    engine.online.forceOutcome({ type: "addressNotAllowed" });
    await fill({ url: "https://user@api.example.com/v1?x=1" });
    await click("Check");
    const title = await screen.findByText(/That address can’t be used\./);
    const message = title.parentElement?.textContent ?? "";
    expect(message).toMatch(/Nothing was sent/);
    expect(message).not.toMatch(/this computer|your own network/);
    expect(screen.getByLabelText(/^Address/)).toHaveAttribute("aria-invalid", "true");
  });

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

  it("send_lead: says where these choices can be changed later", async () => {
    await toSend();
    const lead = screen.getByText(
      sentence(
        "Nocturne only sends what each answer needs. You can change these later in Settings, under Online answers.",
      ),
    );
    expect(chipsIn(lead)).toEqual(["Settings", "Online answers"]);
  });

  it("sensitive_off_by_default: health, money and ID stay here until switched on", async () => {
    await toSend();
    expect(screen.getByRole("switch", { name: "Health, money and ID details" })).not.toBeChecked();
    expect(screen.getByText("Private things are never sent")).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /Private things/ })).toBeNull();
  });

  it("limit_none_default: the limit starts as no limit", async () => {
    await toLimit();
    expect(screen.getByText(/There is no limit\./)).toBeInTheDocument();
    expect(screen.queryByLabelText(/Stop online answers after/)).toBeNull();
    expect(screen.getByRole("button", { name: "Set a limit" })).toBeInTheDocument();
  });

  it("limit_none_default: continuing without choosing sends no limit and asks nothing", async () => {
    const { engine } = await toLimit();
    await click("Continue");
    await heading(STEP5);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      screen.getByText(/There is no limit on how many questions go online/),
    ).toBeInTheDocument();
    const on = vi.spyOn(engine, "turnOnOnline");
    await click("Turn on online answers");
    await waitFor(() => expect(on).toHaveBeenCalledTimes(1));
    expect(on.mock.calls[0]?.[0].limit.kind).toBe("none");
  });

  it("limit_set: a limit can be set, starts at 200 and says when it changed", async () => {
    await toLimit();
    await click("Set a limit");
    const input = screen.getByLabelText(/Stop online answers after/);
    expect(input).toHaveValue("200");
    expect(
      screen.getByText(/Then questions are answered on this computer until the 1st/),
    ).toBeInTheDocument();
    const hint = screen.getByText(
      sentence(
        "Then questions are answered on this computer until the 1st. Settings shows how many are left, under Online answers.",
      ),
    );
    expect(chipsIn(hint)).toEqual(["Settings", "Online answers"]);
    expect(screen.queryByText(/Changed from 200/)).toBeNull();
    expect(
      screen.getByText(/doesn’t tell Nocturne what each one costs, so the limit counts questions/),
    ).toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, "50");
    expect(screen.getByText(/Changed from 200\./)).toBeInTheDocument();
  });

  it("invalid_limit: asks for a whole number and will not go on", async () => {
    await toLimit();
    await click("Set a limit");
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

  it("disabled_reason: a limit that is not a whole number says so in the bar, tied to Continue", async () => {
    await toLimit();
    await click("Set a limit");
    const input = screen.getByLabelText(/Stop online answers after/);
    const line = () => document.querySelector<HTMLElement>("[data-action-bar] span");
    expect(line()).toHaveTextContent("Nothing is turned on until the last step.");
    await userEvent.clear(input);
    await userEvent.type(input, "2.5");
    const next = screen.getByRole("button", { name: "Continue" });
    expect(next).toBeDisabled();
    expect(line()).toHaveTextContent("Continue works once the limit is a whole number, like 200.");
    expect(next).toHaveAccessibleDescription(
      "Continue works once the limit is a whole number, like 200.",
    );
    await userEvent.clear(input);
    await userEvent.type(input, "7");
    expect(line()).toHaveTextContent("Nothing is turned on until the last step.");
    expect(next).not.toHaveAttribute("aria-describedby");
  });

  it("no_limit_direct: going back to no limit needs no confirmation", async () => {
    await toLimit();
    await click("Set a limit");
    expect(screen.getByLabelText(/Stop online answers after/)).toHaveValue("200");
    await userEvent.click(screen.getByRole("button", { name: "Use no limit" }));
    expect(screen.queryByRole("dialog")).toBeNull();
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
    const off = screen.getByText(sentence("Turn it off at any time in Settings or Privacy."));
    expect(chipsIn(off)).toEqual(["Settings", "Privacy"]);
    const receipts = screen.getByText(
      sentence("Every answer says where it was written. See what was sent shows the exact words."),
    );
    expect(chipsIn(receipts)).toEqual(["See what was sent"]);
    await userEvent.click(within(choices).getByRole("button", { name: "Change: Monthly limit" }));
    await heading(STEP4);
  });

  it("consent_what_changes: says what changes, naming the service, before changing your mind", async () => {
    await toLimit();
    await click("Continue");
    await heading(STEP5);
    expect(
      screen.getByText(
        "While this is on, each question and the few lines that answer it leave this computer. Everything else you kept stays here.",
      ),
    ).toBeInTheDocument();
    const section = screen.getByRole("heading", { name: "What changes" })
      .parentElement as HTMLElement;
    expect(section).toHaveTextContent(
      "Your questions are answered by api.example.com, on your account with them, so it may charge for each one. Answers may take a few seconds longer. If it can’t be reached, this computer answers, and says so.",
    );
    const headings = screen
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent)
      .filter((text) => ["Never sent", "What changes", "Changing your mind"].includes(text ?? ""));
    expect(headings).toEqual(["Never sent", "What changes", "Changing your mind"]);
    const bar = document.querySelector<HTMLElement>("[data-action-bar]") as HTMLElement;
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Turn on online answers", "Not now"]);
    expect(bar.querySelector("span")).toBeNull();
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
      phones: false,
      sendSensitive: false,
      limit: { kind: "none", questions: 200, usd: null },
    });
    expect(await screen.findByRole("switch", { name: "Online answers are on" })).toBeChecked();
    expect(window.location.hash).toBe("#/settings/online");
    expect(engine.online.abandoned).toBe(0);
  });

  it("a failed turn on gives one message, offers another go, and turns nothing on", async () => {
    const { engine } = await toLimit();
    await click("Continue");
    await heading(STEP5);
    const on = vi.spyOn(engine, "turnOnOnline").mockRejectedValueOnce(new Error("no"));
    await click("Turn on online answers");
    const alerts = await screen.findAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toHaveTextContent(
      "Nocturne couldn’t save this, so nothing was turned on. Try again.",
    );
    expect(screen.queryByText("That didn’t work. Nothing was changed.")).toBeNull();
    expect(on).toHaveBeenCalledTimes(1);
    expect(engine.online.view().state).toEqual({ type: "off" });
    expect(window.location.hash).toBe("#/settings/online/setup?step=send");
    expect(screen.queryByRole("switch", { name: "Online answers are on" })).toBeNull();
    await click("Try again");
    await waitFor(() => expect(on).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("switch", { name: "Online answers are on" })).toBeChecked();
  });

  it("says plainly when the confirmation was cancelled, and offers the way back", async () => {
    const { engine } = await toLimit();
    await click("Continue");
    await heading(STEP5);
    const on = vi
      .spyOn(engine, "turnOnOnline")
      .mockRejectedValue(new EngineError("declined", "the person said no"));
    await click("Turn on online answers");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "Not turned on. To use this service, check it again from Advanced details.",
    );
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(on).toHaveBeenCalledTimes(1);
    await click("Go to Advanced details");
    expect(window.location.hash).toContain("#/settings/advanced/online");
  });

  const KEY = "sk-example-not-a-real-key";
  const keyAbsent = () => {
    expect(document.body.textContent ?? "").not.toContain(KEY);
    for (const input of document.querySelectorAll("input")) {
      expect((input as HTMLInputElement).value).not.toContain(KEY);
    }
  };

  it("key_not_kept: the key is gone from the page once the check passes, and after Back and on Consent", async () => {
    await toConnect();
    await fill();
    expect(screen.getByLabelText(/^Key/)).toHaveValue(KEY);
    await click("Check");
    await screen.findByText(/It works\./);
    keyAbsent();
    expect(screen.getByLabelText(/^Key/)).toHaveValue("");
    expect(screen.getByRole("button", { name: "Check again" })).toBeDisabled();
    await click("Continue");
    await heading(STEP3);
    keyAbsent();
    await click("Back");
    await heading(STEP2);
    keyAbsent();
    await click("Continue");
    await heading(STEP3);
    await click("Continue");
    await heading(STEP4);
    await click("Continue");
    await heading(STEP5);
    keyAbsent();
    await userEvent.click(screen.getByRole("button", { name: "Change: Service" }));
    await heading(STEP2);
    keyAbsent();
  });

  it("key_not_kept: typing a key again after a pass asks for a new check", async () => {
    await toConnect();
    await fill();
    await click("Check");
    await screen.findByText(/It works\./);
    await enter(/^Key/, KEY);
    expect(screen.queryByText(/It works\./)).toBeNull();
    expect(screen.getByRole("button", { name: "Check" })).toBeEnabled();
  });

  it("race: fields are read-only while checking, and an answer for an old form is dropped", async () => {
    const { engine } = await toConnect();
    await fill();
    let settle: (o: CheckOutcome) => void = () => undefined;
    vi.spyOn(engine, "checkOnline").mockImplementationOnce(
      () => new Promise<CheckOutcome>((resolve) => (settle = resolve)),
    );
    await click("Check");
    for (const label of [/^Address/, /^Key/, /^Which one to use/]) {
      expect(screen.getByLabelText(label)).toHaveAttribute("readonly");
    }
    fireEvent.change(screen.getByLabelText(/^Which one to use/), {
      target: { value: "another-name" },
    });
    settle({ type: "works", host: "api.example.com", model: "llama-3.3-70b-instruct" });
    await waitFor(() => expect(screen.queryByText(/Checking with/)).toBeNull());
    expect(screen.queryByText(/It works\./)).toBeNull();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("focus: stays on the step while checking and when the result arrives", async () => {
    const { engine } = await toConnect();
    await fill();
    engine.online.setCheckMs(50);
    await click("Check");
    const region = screen.getByText(/Checking with/).closest("[role=status]") as HTMLElement;
    expect(region).toHaveFocus();
    await screen.findByText(/It works\./);
    expect(region).toHaveFocus();
  });

  it("errors are tied to their fields, the name field included", async () => {
    const { engine } = await toConnect();
    engine.online.forceOutcome({ type: "modelMissing" });
    await fill();
    await click("Check");
    await screen.findByText(/doesn’t know a name like/);
    const model = screen.getByLabelText(/^Which one to use/);
    expect(model).toHaveAccessibleDescription(/doesn’t know a name like/);
    expect(screen.getByLabelText(/^Key/)).not.toHaveAccessibleDescription(/doesn’t know a name/);
  });

  it("invalid count in money mode says the count message, not the amount one", async () => {
    await toLimit("cost");
    await userEvent.click(screen.getByRole("radio", { name: /A number of questions/ }));
    const input = screen.getByLabelText("A number of questions", { selector: "input[inputmode]" });
    await userEvent.clear(input);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Enter a whole number from 1 upwards, like 200.",
    );
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

  it("Change… opens the form in Advanced details with the address and name filled in, and no key", async () => {
    await openOnline(ADVANCED, "on", CONNECT);
    expect(screen.getByLabelText(/^Address/)).toHaveValue("https://api.example.com/v1");
    expect(screen.getByLabelText(/^Which one to use/)).toHaveValue("llama-3.3-70b-instruct");
    expect(screen.getByLabelText(/^Key/)).toHaveValue("");
  });
});
