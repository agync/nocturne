import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CheckOutcome } from "../../engine/bindings";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { hostOf } from "./mind";
import { resetScriptedMind, scriptedMind, scriptedNoteRows } from "./testing/homeMock";

afterEach(async () => {
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  scriptedNoteRows.clear();
  resetScriptedMind();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

const nav = () => screen.getByRole("navigation", { name: "Nocturne" });
const button = (name: string | RegExp) => screen.getByRole("button", { name });

async function toDetails() {
  fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
  await screen.findByRole("heading", { name: "Where should Nocturne’s mind be?" });
  fireEvent.click(button("Continue"));
  await screen.findByRole("heading", {
    name: "Have Nocturne’s mind work online, with a service you choose.",
  });
  fireEvent.click(button("Continue"));
  await screen.findByRole("heading", { name: "Your service’s details" });
}

function fill(url: string, key: string, model: string) {
  fireEvent.change(screen.getByLabelText("API URL"), { target: { value: url } });
  fireEvent.change(screen.getByLabelText("API key"), { target: { value: key } });
  fireEvent.change(screen.getByLabelText("Model name"), { target: { value: model } });
}

describe("Where should Nocturne's mind be?", () => {
  it("offers the two ways of the first release, the online service first", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
    expect(
      await screen.findByRole("heading", { name: "Where should Nocturne’s mind be?" }),
    ).toBeVisible();
    expect(
      screen.getByText(/Your memory stays on this phone unless you move it/),
    ).toBeInTheDocument();
    const ways = screen.getAllByRole("radio");
    expect(ways).toHaveLength(2);
    expect(ways[0]).toHaveAccessibleName(/^An online service you choose/);
    expect(ways[0]).toBeChecked();
    expect(ways[1]).toHaveAccessibleName(/^A computer of your own/);
    expect(screen.queryByText(/Apple Intelligence|A computer at home/)).toBeNull();
  });
});

describe("An online service you choose", () => {
  it("says what goes, with everything sent behind the expander", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Nocturne’s mind online · step 1 of 4")).toBeVisible();
    expect(screen.getByText(/your journal and check-ins/)).toBeInTheDocument();
    expect(screen.getByText("Nothing is turned on until the last step.")).toBeInTheDocument();
    const more = button("Everything that is sent");
    expect(more).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByText("Kept 7:40 am · Dentist moved to Thursday at 9, Bridge Street."),
    ).toBeVisible();
    expect(screen.getByText(/This phone sends it straight to the service/)).toBeInTheDocument();
  });

  it("checks the details, naming each problem, and continues once it works", async () => {
    await app("m=fresh");
    await toDetails();
    const next = button("Continue");
    expect(next).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText("Continue works once the check passes.")).toBeInTheDocument();
    expect(button("Check")).toHaveAttribute("aria-disabled", "true");

    fill("https://offline.example/v1", "abc", "reader-small");
    fireEvent.click(button("Check"));
    expect(await screen.findByText("This phone isn’t online.")).toBeInTheDocument();

    fill("https://answers.example/v1", "refused", "reader-small");
    fireEvent.click(button("Check"));
    expect(await screen.findByText("answers.example refused the key.")).toBeInTheDocument();

    fill("https://answers.example/v1", "abc", "missing");
    fireEvent.click(button("Check"));
    expect(
      await screen.findByText("answers.example doesn’t know a name like “missing”."),
    ).toBeInTheDocument();
    expect(button("Check again")).toBeInTheDocument();

    fill("https://answers.example/v1", "abc", "reader-small");
    fireEvent.click(button("Check"));
    expect(await screen.findByText("answers.example answered. It works.")).toBeInTheDocument();
    expect(button("Continue")).not.toHaveAttribute("aria-disabled");
    expect(screen.queryByText("Continue works once the check passes.")).toBeNull();

    fireEvent.change(screen.getByLabelText("Model name"), { target: { value: "reader-big" } });
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
  });

  it("sets a limit, agrees with every fact, turns on and shows the reading", async () => {
    await app("m=kept");
    await toDetails();
    fill("https://answers.example/v1", "abc", "reader-small");
    fireEvent.click(button("Check"));
    await screen.findByText("answers.example answered. It works.");
    fireEvent.click(button("Continue"));

    expect(
      await screen.findByRole("heading", { name: "How much may go online each month?" }),
    ).toBeVisible();
    expect(screen.getByRole("radio", { name: "No limit" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "A number of things read and answered" }));
    const count = screen.getByLabelText("Things read and answered in a month");
    fireEvent.change(count, { target: { value: "none" } });
    expect(
      screen.getByText("Continue works once the limit is a whole number, like 200."),
    ).toBeInTheDocument();
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
    fireEvent.change(count, { target: { value: "150" } });
    fireEvent.click(button("Continue"));

    expect(
      await screen.findByRole("heading", {
        name: "Before you turn this on, here is exactly what happens.",
      }),
    ).toBeVisible();
    expect(
      screen.getByText(/go from this phone to answers\.example, so it can read them/),
    ).toBeInTheDocument();
    expect(screen.getByText(/What it uses, reader-small, may be made/)).toBeInTheDocument();
    expect(screen.getByText(/At most 150 things a month are read and answered/)).toBeVisible();
    expect(screen.getByText(/Turn it off at any time in/)).toHaveTextContent(
      "Turn it off at any time in This phone. What was already read stays in your memory on this phone.",
    );
    expect(screen.getByText("Things you let go are never sent.", { exact: false })).toBeVisible();

    fireEvent.click(button("Turn on"));
    expect(await screen.findByRole("heading", { name: "Reading what you kept" })).toBeVisible();
    expect(screen.getByText(/Nocturne is reading 46 things you kept/)).toBeInTheDocument();
    expect(screen.getByText("12 of 46 read")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "12");

    fireEvent.click(button("OK"));
    expect(await screen.findByText("Written by answers.example")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "The swap for the 28th is still open." }),
    ).toBeVisible();
    expect(
      screen.queryByRole("region", { name: "Your page needs a mind to write it." }),
    ).toBeNull();
  });

  it("leaves the whole way in with Not now", async () => {
    await app("m=fresh");
    fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    await screen.findByText("Nocturne’s mind online · step 1 of 4");
    fireEvent.click(button("Not now"));
    expect(
      await screen.findByRole("region", { name: "Your page needs a mind to write it." }),
    ).toBeInTheDocument();
  });

  it("keeps No limit too, so it lifts a limit set before", async () => {
    history.replaceState(null, "", "/?m=kept");
    const services = await nativeMockServices();
    const real = services.mind;
    if (!real) throw new Error("scripted mind");
    const setLimit = vi.fn(real.setLimit);
    services.mind = { ...real, setLimit };
    render(<NativeRoot services={services} />);
    await toDetails();
    fill("https://answers.example/v1", "abc", "reader-small");
    fireEvent.click(button("Check"));
    await screen.findByText("answers.example answered. It works.");
    fireEvent.click(button("Continue"));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(setLimit).toHaveBeenCalledWith({ kind: "none" }));
  });

  it("drops a check that answers for details since changed", async () => {
    history.replaceState(null, "", "/?m=fresh");
    const services = await nativeMockServices();
    const real = services.mind;
    if (!real) throw new Error("scripted mind");
    let answer: (o: CheckOutcome) => void = () => undefined;
    services.mind = {
      ...real,
      check: () =>
        new Promise<CheckOutcome>((done) => {
          answer = done;
        }),
    };
    render(<NativeRoot services={services} />);
    await toDetails();
    fill("https://answers.example/v1", "old", "reader-small");
    fireEvent.click(button("Check"));
    await screen.findByText("Checking with answers.example…");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "new" } });
    answer({ type: "works", host: "answers.example", model: "reader-small" });
    await new Promise((settle) => setTimeout(settle, 20));
    expect(screen.queryByText("answers.example answered. It works.")).toBeNull();
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
  });

  it("keeps the key out of the history entries", async () => {
    await app("m=fresh");
    await toDetails();
    fill("https://answers.example/v1", "secret-key-123", "reader-small");
    fireEvent.click(button("Check"));
    await screen.findByText("answers.example answered. It works.");
    fireEvent.click(button("Continue"));
    await screen.findByRole("heading", { name: "How much may go online each month?" });
    expect(JSON.stringify(history.state)).not.toContain("secret-key-123");
  });

  it("goes back to Today when the reading can't be shown", async () => {
    history.replaceState(null, "", "/?m=fresh");
    const services = await nativeMockServices();
    const real = services.mind;
    if (!real) throw new Error("scripted mind");
    services.mind = { ...real, reading: () => Promise.reject(new Error("no")) };
    render(<NativeRoot services={services} />);
    await toDetails();
    fill("https://answers.example/v1", "abc", "reader-small");
    fireEvent.click(button("Check"));
    await screen.findByText("answers.example answered. It works.");
    fireEvent.click(button("Continue"));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Turn on" }));
    await waitFor(() =>
      expect(
        screen.queryByRole("region", { name: "Your page needs a mind to write it." }),
      ).toBeNull(),
    );
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
  });
});

describe("The mind in This phone", () => {
  it("lists the service, what goes and this month, and turns off", async () => {
    await app("m=kept&mind=on");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    const group = await screen.findByRole("region", { name: "Nocturne’s mind online" });
    expect(
      within(group).getByText("answers.example, online · Reading · 12 of 46"),
    ).toBeInTheDocument();
    expect(within(group).getByText("What you keep, and questions")).toBeInTheDocument();
    expect(
      await within(group).findByText("3 things read and answered · limit 200"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Its mind.*None yet/ })).toBeNull();

    fireEvent.click(within(group).getByRole("button", { name: "Turn off…" }));
    const sheet = await screen.findByRole("dialog", { name: "Turn off Nocturne’s mind online?" });
    expect(
      within(sheet).getByText(
        "What was read stays in your memory. What you keep from now waits to be read.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Turn off" }));
    expect(await screen.findByRole("button", { name: /Its mind.*None yet/ })).toBeVisible();
    expect(screen.queryByRole("region", { name: "Nocturne’s mind online" })).toBeNull();
  });

  it("opens the Sent online list: what each was for, when and how much, never its words", async () => {
    await app("m=kept&mind=on");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "See what was sent" }));
    expect(
      await screen.findByText(/Everything this phone sent to answers\.example/),
    ).toBeInTheDocument();
    expect(await screen.findByText("Read a thing you kept")).toBeInTheDocument();
    expect(screen.getByText("Wrote your page")).toBeInTheDocument();
    expect(screen.getByText("Connection check")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Saturday 21 November" })).toBeInTheDocument();
    expect(screen.getByText("Older than 90 days are removed by themselves.")).toBeInTheDocument();
    expect(screen.getByText("1.8 kB", { exact: false })).toBeInTheDocument();
    expect(screen.queryByText("Exactly what was sent")).toBeNull();
    expect(document.querySelector("details")).toBeNull();
  });

  it("says so when the Sent online list can't be read", async () => {
    history.replaceState(null, "", "/?m=kept&mind=on");
    const services = await nativeMockServices();
    const real = services.mind;
    if (!real) throw new Error("scripted mind");
    services.mind = { ...real, sent: () => Promise.reject(new Error("no")) };
    render(<NativeRoot services={services} />);
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "See what was sent" }));
    expect(
      await screen.findByText("Nocturne couldn’t read this list. Nothing was changed."),
    ).toBeInTheDocument();
  });

  it("asks for the key again when it isn't held, and Move my memory still scans", async () => {
    await app("m=kept&mind=needsKey");
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    expect(await screen.findByText("Nocturne can’t read the saved key")).toBeInTheDocument();
    fireEvent.click(button("Move my memory…"));
    expect(await screen.findByRole("heading", { name: "Give Nocturne a mind" })).toBeVisible();
    fireEvent.click(button("Not now"));
    fireEvent.click(await screen.findByRole("button", { name: "Check the key…" }));
    expect(await screen.findByRole("heading", { name: "Your service’s details" })).toBeVisible();
    expect(screen.getByText("Nocturne’s mind online · step 2 of 4")).toBeInTheDocument();
  });
});

describe("Today and Ask with a mind", () => {
  it("shows the page it wrote on Today, and no card asking for a mind", async () => {
    await app("m=kept&mind=on");
    expect(await screen.findByText("Written by answers.example")).toBeInTheDocument();
    expect(screen.getByText("You meant to ask about the swap for the 28th.")).toBeVisible();
    expect(screen.queryByText("Your page needs a mind to write it.")).toBeNull();
  });

  it("shows nothing in the page's place while the first page is being written", async () => {
    await app("m=kept&mind=needsKey");
    expect(await screen.findByText("Your memory is on this phone")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/Kept today/)).toBeInTheDocument());
    expect(screen.queryByText("Your page needs a mind to write it.")).toBeNull();
    expect(screen.queryByText(/Written by/)).toBeNull();
  });

  it("answers a question from the phone's own mind, with its receipt", async () => {
    await app("m=kept&mind=on");
    await screen.findByText("Written by answers.example");
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Your question" }), {
      target: { value: "What did I mean to ask?" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Ask" }));
    expect(
      await screen.findByText("You meant to ask about the swap for the 28th."),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("Answered online by your provider (answers.example)"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/listed on your computer|on your computer/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Choose a mind" })).toBeNull();
  });

  it("finds by words, saying answers are back when the service is", async () => {
    await app("m=kept&mind=on&trouble=unreachable");
    expect(
      await screen.findByText(/Written Saturday 21 November\. answers\.example hasn’t answered/),
    ).toBeInTheDocument();
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText("Answers are back when answers.example is.")).toBeVisible();
    fireEvent.change(await screen.findByRole("textbox", { name: "Find something you kept" }), {
      target: { value: "swap" },
    });
    expect(await screen.findByText("Found by their words.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Choose a mind" })).toBeNull();
  });

  it("names a key the service refused, in Today and Ask, and offers the check", async () => {
    await app("m=kept&mind=on&trouble=keyRefused");
    expect(await screen.findByText("Your provider refused the key")).toBeVisible();
    expect(
      screen.getByText(
        "answers.example no longer accepts the key Nocturne has. Until you check it, Ask finds by words.",
      ),
    ).toBeVisible();
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText("Your provider refused the key")).toBeVisible();
    fireEvent.click(button("Check the key…"));
    expect(await screen.findByRole("heading", { name: "Your service’s details" })).toBeVisible();
  });

  it("tells a key the phone no longer holds on Today and Ask", async () => {
    await app("m=kept&mind=needsKey");
    expect(await screen.findByText("Nocturne can’t read the saved key")).toBeVisible();
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText("Nocturne can’t read the saved key")).toBeVisible();
    fireEvent.click(button("Check the key…"));
    expect(await screen.findByRole("heading", { name: "Your service’s details" })).toBeVisible();
  });

  it("says when there is no credit, and when the limit has paused it", async () => {
    await app("m=kept&mind=on&trouble=noCredit");
    expect(await screen.findByText("Your provider says there’s no credit left")).toBeVisible();
    expect(
      screen.getByText(
        "Add credit on your provider’s website, then check again. Until then, Ask finds by words.",
      ),
    ).toBeVisible();
    cleanup();
    history.replaceState(null, "", "/");
    await app("m=kept&mind=on&trouble=paused");
    expect(await screen.findByText("Paused by your limit")).toBeVisible();
    expect(
      screen.getByText(
        "You’ve reached this month’s limit. Ask finds by words until Tuesday 1 December.",
      ),
    ).toBeVisible();
  });
});

describe("When the service or the phone is not online", () => {
  it("says this phone isn't online on Today and Ask, and finds by words", async () => {
    await app("m=kept&mind=on&trouble=offline");
    const line = "This phone isn’t online. What you keep waits to be read.";
    expect(await screen.findByText(line)).toBeVisible();
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText(line)).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Find something you kept" })).toBeVisible();
    expect(screen.queryByText(/Answers are back/)).toBeNull();
  });

  it("turns Ask back to the mind when the service answers again", async () => {
    await app("m=kept&mind=on&trouble=unreachable");
    await screen.findByText(/answers\.example hasn’t answered/);
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(await screen.findByText("Answers are back when answers.example is.")).toBeVisible();
    scriptedMind.trouble = null;
    await emit("mind-status");
    expect(await screen.findByRole("textbox", { name: "Your question" })).toBeVisible();
    expect(screen.queryByText("Answers are back when answers.example is.")).toBeNull();
  });
});

describe("The answer screen on a phone alone", () => {
  async function ask(question: string) {
    await app("m=kept&mind=on");
    await screen.findByText("Written by answers.example");
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Your question" }), {
      target: { value: question },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Ask" }));
  }

  it("points to this phone's own Sent online list after an answer, and nowhere else", async () => {
    await ask("What did I mean to ask?");
    await screen.findByText("Answered online by your provider (answers.example)");
    const line = screen.getByText(
      (_, el) =>
        el?.tagName === "P" &&
        el.textContent === "What was sent is listed in This phone. Choose See what was sent.",
    );
    expect(line.querySelectorAll(".ph-cc")).toHaveLength(2);
    expect(screen.queryByText(/computer|Correct it/)).toBeNull();
  });

  it("says when no answer came, without saying it is getting ready", async () => {
    await ask("no answer for this");
    expect(
      await screen.findByText("No answer came this time. Ask again in a moment."),
    ).toBeVisible();
    expect(screen.queryByText(/getting ready|computer/)).toBeNull();
  });

  it("says how to tell which is right when not sure", async () => {
    await ask("which day is the swap?");
    expect(await screen.findByText("Open each one to see which is right.")).toBeVisible();
    expect(screen.queryByText(/computer/)).toBeNull();
  });

  it("stops a question: the request is dropped and nothing is shown", async () => {
    await ask("tell me slowly");
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(await screen.findByText("Stopped.")).toBeVisible();
    await waitFor(() => expect(scriptedMind.stopped).toHaveLength(1));
    await new Promise((settle) => setTimeout(settle, 20));
    expect(screen.queryByText("No answer came this time. Ask again in a moment.")).toBeNull();
    expect(screen.getByText("Stopped.")).toBeVisible();
  });
});

describe("Delete everything with a mind", () => {
  it("erases the mind in the one step, without turning it off first", async () => {
    history.replaceState(null, "", "/?m=kept&s=welcome&mind=on");
    const services = await nativeMockServices();
    const real = services.mind;
    if (!real) throw new Error("scripted mind");
    const off = vi.fn(real.turnOff);
    services.mind = { ...real, turnOff: off };
    render(<NativeRoot services={services} />);
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
    fireEvent.change(await screen.findByLabelText("Type “delete everything” to go ahead"), {
      target: { value: "delete everything" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete everything" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeVisible();
    expect(off).not.toHaveBeenCalled();
  });
});

describe("hostOf", () => {
  it("names the host, or the words as typed", () => {
    expect(hostOf(" https://answers.example/v1 ")).toBe("answers.example");
    expect(hostOf("http://192.168.1.20:8080/v1")).toBe("192.168.1.20:8080");
    expect(hostOf("not an address")).toBe("not an address");
  });
});
