import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const open = async (engine = createMockEngine()) => {
  renderApp({ hash: "#/pages", engine });
  await screen.findByRole("heading", { level: 1, name: "Past pages" });
  await screen.findByRole("heading", { level: 2, name: "November 2026" });
  return engine;
};

describe("Past pages", () => {
  it("lists every page by date, newest first, with its headline", async () => {
    await open();
    const links = screen.getAllByRole("link", { name: /Page \d+/ });
    expect(links[0]).toHaveAttribute("href", "#/pages/2026-11-13");
    expect(links[0]).toHaveTextContent("Today");
    expect(links[0]).toHaveTextContent("Fri 13 Nov");
    expect(links[0]).toHaveTextContent("Two things need you today.");
    expect(links[0]).toHaveTextContent("Page 23");
    const dates = links.map((l) => l.getAttribute("href"));
    expect(dates).toEqual([...dates].sort().reverse());
  });

  it("groups pages by month", async () => {
    await open();
    expect(screen.getByRole("heading", { level: 2, name: "November 2026" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "October 2026" })).toBeInTheDocument();
  });

  it("says how many pages still need you, in words, only where some do", async () => {
    await open();
    const today = screen.getByRole("link", { name: /Two things need you today/ });
    expect(within(today).getByText("2 still need you")).toBeInTheDocument();
    const quiet = screen.getAllByRole("link", { name: /A quiet night/ })[0] as HTMLElement;
    expect(within(quiet).queryByText(/still need/)).toBeNull();
  });

  it("opens a page on the morning page screen", async () => {
    await open();
    await userEvent.click(screen.getByRole("link", { name: /Your first page/ }));
    expect(window.location.hash).toBe("#/pages/2026-10-31");
    expect(
      await screen.findByRole("heading", { level: 1, name: "Your first page." }),
    ).toBeInTheDocument();
  });

  it("has a calm empty state", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "listPages").mockResolvedValue([]);
    renderApp({ hash: "#/pages", engine });
    expect(await screen.findByText(/No pages yet/)).toBeInTheDocument();
  });

  it("says so and offers to try again when the list cannot be read", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "listPages").mockRejectedValueOnce(new Error("no"));
    renderApp({ hash: "#/pages", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nocturne couldn’t open your pages.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "November 2026" }),
    ).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
