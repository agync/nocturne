import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import { askPlace } from "../../components/KeepAskBar/askPlace";
import type { AskPlace } from "../../engine/bindings";
import type { OnlineScenario } from "../../engine/mock/online";
import { renderApp, setViewport } from "../../test/render";
import { askSession } from "./session";
import { oneMemoryAnswer, ScriptedAnswers } from "./testing";

const QUESTION = "When is Leo’s swimming?";
const TITLE = "Online answers has changed";

beforeEach(() => {
  askSession.reset();
  setViewport(1280);
  askPlace.take();
});
afterEach(() => {
  askSession.reset();
  window.location.hash = "";
  askRequests.take();
});

/** A computer in the given Online answers state, asked one question from the bar. */
async function open(scenario: OnlineScenario, place: AskPlace = "auto") {
  const engine = new ScriptedAnswers();
  engine.online.setScenario(scenario);
  engine.steps = await oneMemoryAnswer(engine, {
    phase: "done",
    answeredBy: { type: "online", host: "api.example.com" },
  });
  askPlace.set(place);
  askRequests.request(QUESTION);
  renderApp({ hash: "#/ask", engine });
  return engine;
}

describe("the notice for someone who agreed before", () => {
  it("holds the question until Continue, then sends it once and never shows again", async () => {
    const engine = await open("changed");
    const see = vi.spyOn(engine, "seeOnlineChange");
    expect(await screen.findByRole("heading", { name: TITLE })).toHaveFocus();
    expect(
      screen.getByText(
        "Nocturne no longer keeps back lines that look like health, money or ID details. Anything you kept can now go with a question it helps answer, unless you marked it private.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Nocturne doesn’t guess what is private. Mark anything you never want sent as private.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Your question waits until you continue.")).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(engine.asked).toEqual([]);

    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(engine.asked).toEqual([{ question: QUESTION, place: "auto" }]));
    expect(see).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("heading", { name: TITLE })).toBeNull();
    expect(engine.online.view().changeNotice).toBe(false);
  });

  it("answers on this computer, records nothing, and comes back with the next question", async () => {
    const engine = await open("changed");
    const see = vi.spyOn(engine, "seeOnlineChange");
    await screen.findByRole("heading", { name: TITLE });
    const notice = screen.getByRole("heading", { name: TITLE }).closest("section");
    expect([...(notice?.querySelectorAll("button") ?? [])].map((b) => b.textContent)).toEqual([
      "Continue",
      "Answer on this computer",
      "Online answers settings",
    ]);
    await userEvent.click(screen.getByRole("button", { name: "Answer on this computer" }));
    await waitFor(() =>
      expect(engine.asked).toEqual([{ question: QUESTION, place: "thisComputer" }]),
    );
    expect(see).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: TITLE })).toBeNull();
    expect(engine.online.view().changeNotice).toBe(true);
  });

  it("leads to Online answers in Settings and sends nothing", async () => {
    const engine = await open("changed");
    await screen.findByRole("heading", { name: TITLE });
    await userEvent.click(screen.getByRole("button", { name: "Online answers settings" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/online"));
    expect(engine.asked).toEqual([]);
    expect(engine.online.view().changeNotice).toBe(true);
  });

  it("is not shown for a question kept on this computer", async () => {
    const engine = await open("changed", "thisComputer");
    await waitFor(() =>
      expect(engine.asked).toEqual([{ question: QUESTION, place: "thisComputer" }]),
    );
    expect(screen.queryByRole("heading", { name: TITLE })).toBeNull();
  });

  it("is not shown to someone who set up with the new words", async () => {
    const engine = await open("on");
    await waitFor(() => expect(engine.asked).toHaveLength(1));
    expect(screen.queryByRole("heading", { name: TITLE })).toBeNull();
  });
});
