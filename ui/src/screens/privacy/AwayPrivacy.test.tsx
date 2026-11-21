import { screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const open = async (scenario?: string, sentMeasured = true) => {
  const engine = createMockEngine({ sentMeasured, remote: { auto: false, scenario } });
  renderApp({ hash: "#/privacy", engine });
  await screen.findByRole("heading", { level: 1 });
};

describe("privacy", () => {
  it("away_on_simple_names_cloudflare_and_what_it_sees: says who is in the middle and what it can and cannot see", async () => {
    await open("on");
    const section = await screen.findByRole("region", { name: "Away from home" });
    expect(section).toHaveTextContent(/Your phones reach this computer through Cloudflare/);
    expect(section).toHaveTextContent(
      "Your phones can bring their calendar and health readings, in the same locked messages.",
    );
    expect(section).toHaveTextContent(/locked so Cloudflare can’t read it/);
    expect(
      within(section).getByRole("heading", { name: "The company in the middle: Cloudflare" }),
    ).toBeInTheDocument();
    expect(within(section).getByText("When your phone connects")).toBeInTheDocument();
    expect(
      within(section).getByText(/internet address, and roughly where it is/),
    ).toBeInTheDocument();
    expect(within(section).getByText("How much goes back and forth, each way")).toBeInTheDocument();
    expect(
      within(section).getByText(
        /It serves the page your phone opens, and a change it made could stay/,
      ),
    ).toBeInTheDocument();
    for (const line of ["What you ask", "What you keep", "Your answers and pages"]) {
      expect(within(section).getByText(line)).toBeInTheDocument();
    }
    expect(within(section).getByRole("heading", { name: "Never on a phone" })).toBeInTheDocument();
    expect(
      within(section).getByText(/A stable id for each phone, and the size and timing/),
    ).toBeInTheDocument();
    expect(within(section).getByRole("heading", { name: "Worth knowing" })).toBeInTheDocument();
    expect(section).toHaveTextContent(
      /unlocked or stolen phone can read everything not marked private/,
    );
    expect(section).toHaveTextContent(/only comparing the three words stops it/);
    expect(section).toHaveTextContent(
      /In Simple, anyone using this computer can see the public address/,
    );
    expect(within(section).getByRole("button", { name: "Turn off" })).toBeInTheDocument();
    expect(within(section).getByRole("link", { name: "Away from home settings" })).toHaveAttribute(
      "href",
      "#/settings/away",
    );
  });

  it("off_after_on_shows_last_on: says it is off and when it was last on", async () => {
    await open("offBefore");
    const section = await screen.findByRole("region", { name: "Away from home" });
    expect(section).toHaveTextContent(/Away from home is off\. It was last on yesterday, 21:10\./);
    expect(within(section).queryByRole("button", { name: "Turn off" })).toBeNull();
  });

  it("shows nothing about Away from home when it was never on", async () => {
    await open("off");
    expect(screen.queryByRole("region", { name: "Away from home" })).toBeNull();
    expect(screen.getByRole("heading", { level: 1, name: "Nothing." })).toBeInTheDocument();
  });

  it("no_live_measure_claim: never claims nothing was sent, or live watching, while phones are reached", async () => {
    await open("on", true);
    expect(screen.queryByText("Nothing.")).toBeNull();
    expect(screen.queryByText(/Watching live/)).toBeNull();
    expect(screen.queryByText("Not measured.")).toBeNull();
    expect(
      screen.getByRole("heading", {
        level: 1,
        name: "Only downloads you agreed to, and Away from home.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/can’t count what the connection part itself sends to Cloudflare/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/can’t tell Nocturne what it sends/)).toBeNull();
    expect(screen.getAllByText("3.2 MB").length).toBeGreaterThan(0);
    expect(screen.getByText("Counted by Nocturne as it leaves this computer")).toBeInTheDocument();
  });

  it("ledger_shows_helper_and_on_off: lists the downloaded part and each time it was turned on", async () => {
    await open("on");
    const ledger = screen.getByRole("region", { name: "Downloaded, with your say-so" });
    expect(within(ledger).getByText("Connection part from Cloudflare")).toBeInTheDocument();
    expect(within(ledger).getByText(/^40 MB · \d+ \w+$/)).toBeInTheDocument();
    const row = within(ledger).getByText("Away from home on").closest("li");
    expect(row?.textContent).toMatch(/Away from home on\d+ \w{3}$/);
    expect(row?.textContent).not.toMatch(/MB|0 B/);
  });

  it("only says it cannot tell what is sent when measuring is unavailable", async () => {
    await open("on", false);
    expect(screen.getByText(/can’t tell Nocturne what it sends/)).toBeInTheDocument();
  });

  it("headline_says_away_from_home_is_on: never says only downloads go out while phones are reached", async () => {
    await open("on", true);
    expect(
      screen.getByRole("heading", {
        level: 1,
        name: "Only downloads you agreed to, and Away from home.",
      }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 1, name: "Nothing." })).toBeNull();
    expect(
      screen.getByText(/can’t count what the connection part itself sends/),
    ).toBeInTheDocument();
  });

  it("the promise says what Away from home uses", async () => {
    await open("off");
    expect(screen.getByText(/Cloudflare or Tailscale to reach your phone/)).toBeInTheDocument();
  });
});
