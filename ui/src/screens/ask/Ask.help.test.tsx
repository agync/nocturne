import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askRequests } from "../../app/ask-request";
import { askPlace } from "../../components/KeepAskBar/askPlace";
import { createMockEngine } from "../../engine/mock/engine";
import { onlineViewOn } from "../../engine/mock/onlineAsk";
import { renderApp, setViewport } from "../../test/render";
import { askSession } from "./session";

const LONG = { timeout: 5000 };

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

function open(question: string, helpWritten: boolean) {
  const engine = createMockEngine({ helpWritten });
  askRequests.request(question);
  renderApp({ hash: "#/ask", engine });
  return engine;
}

describe("help with Nocturne", () => {
  it("shows the guide's own words on a size that has not passed the help check", async () => {
    open("How do I get Nocturne on my phone?", false);
    const card = (await screen.findByText("From the Nocturne guide", {}, LONG)).closest("section");
    expect(card).not.toBeNull();
    const words = within(card as HTMLElement);
    expect(words.getByRole("heading", { name: "Turn it on" })).toBeInTheDocument();
    expect(words.getByRole("heading", { name: "Add a phone" })).toBeInTheDocument();
    expect(words.getByText(/lets your phone ask Nocturne a question/)).toBeInTheDocument();
    expect(words.getByRole("button", { name: "Open Away from home" })).toBeInTheDocument();
    expect(words.getByRole("button", { name: "Open the guide" })).toBeInTheDocument();
    expect(
      words.getByRole("link", { name: "Read “Add a phone” in the Nocturne guide" }),
    ).toHaveAttribute("href", "#/guide?page=away-from-home&section=add-a-phone&from=ask");
    expect(screen.getByText("From what you kept")).toBeInTheDocument();
    expect(screen.getByText("Nothing you’ve kept is about this.")).toBeInTheDocument();
    expect(screen.queryByText("Nothing you’ve kept answers that.", { selector: "p" })).toBeNull();
  });

  it("writes one answer from the guide on a size that passed, with guide receipts and buttons", async () => {
    open("How do I get Nocturne on my phone?", true);
    expect(
      await screen.findByText("Answered from the Nocturne guide", {}, LONG),
    ).toBeInTheDocument();
    const receipt = screen.getByRole("link", { name: "Read “Turn it on” in the Nocturne guide" });
    expect(receipt).toHaveTextContent("Nocturne guide · Away from home");
    expect(receipt.closest("li")?.querySelector("[data-chip]")).toBeNull();
    expect(document.querySelector("article [data-chip]")).not.toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open the guide" }));
    expect(window.location.hash).toBe("#/guide?page=away-from-home&section=turn-it-on&from=ask");
  });

  it("says help is always written here when Online answers is on, and sends nothing", async () => {
    const engine = createMockEngine({ helpWritten: true });
    vi.spyOn(engine.online, "view").mockReturnValue(onlineViewOn());
    const record = vi.spyOn(engine.online, "record");
    askRequests.request("How do I take Nocturne off my computer?");
    renderApp({ hash: "#/ask", engine });
    expect(
      await screen.findByText(
        "Answered on this computer from the Nocturne guide · help is always written here",
        {},
        LONG,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "See what was sent" })).toBeNull();
    expect(record).not.toHaveBeenCalled();
  });

  it("asks whether help was meant when unsure, and answers from the guide when asked", async () => {
    const engine = open("Can I read my page on the train?", true);
    const ask = vi.spyOn(engine, "ask");
    expect(
      await screen.findByText("Did you mean help with Nocturne?", {}, LONG),
    ).toBeInTheDocument();
    expect(
      screen.getByText("The Nocturne guide covers this in Away from home."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Nothing you’ve kept answers that.", { selector: "p" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show the guide’s answer" }));
    expect(ask).toHaveBeenCalledWith("Can I read my page on the train?", "auto", true);
    expect(
      await screen.findByText("Answered from the Nocturne guide", {}, LONG),
    ).toBeInTheDocument();
    expect(screen.queryByText("Did you mean help with Nocturne?")).toBeNull();
    await waitFor(() => expect(screen.queryByText("Earlier")).toBeNull());
  });

  it("leaves a memory question that shares words with the guide alone", async () => {
    open("When is Leo’s swimming?", true);
    await screen.findByRole("button", { name: "That’s wrong" }, LONG);
    expect(screen.queryByText(/Nocturne guide/)).toBeNull();
    expect(screen.queryByText("Did you mean help with Nocturne?")).toBeNull();
  });

  it("offers a help question on the empty Ask screen", async () => {
    renderApp({ hash: "#/ask", engine: createMockEngine() });
    expect(
      await screen.findByRole("button", { name: "How do I use Nocturne on my phone?" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Nocturne answers only from what it can show you, with where each answer came from underneath. It can also help you use Nocturne.",
      ),
    ).toBeInTheDocument();
  });
});
