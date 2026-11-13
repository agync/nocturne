import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PageView } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { createMockEngine } from "../../engine/mock/engine";
import { PRINT_STRESS_DATE } from "../../engine/mock/seed/print";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { fit } from "./fit";
import { pageRule, pixelsToMm, Sheet } from "./Print";

beforeEach(() => {
  setViewport(1280);
  window.localStorage.clear();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  window.location.hash = "";
});

const stress = async (): Promise<PageView> =>
  createMockEngine().getEdition(PRINT_STRESS_DATE, "print");

const sheet = () => screen.getByRole("article", { name: "Print" });

describe("what fits on one sheet", () => {
  it("fit_order_caps_lines", async () => {
    const page = await stress();
    const a4 = fit(page, "a4");
    expect(a4.needsYou).toHaveLength(6);
    expect(a4.needsYouMore).toBe(8 - 6 + page.moreNeedsYou);
    expect(a4.changed).toHaveLength(6);
    expect(a4.changedMore).toBe(9 - 6 + page.moreChanged);
    const letter = fit(page, "letter");
    expect(letter.changed).toHaveLength(5);
    expect(letter.changedMore).toBe(9 - 5 + page.moreChanged);
    expect(a4.letGo).toEqual([
      "The old gym door code",
      "The pizza place on Elm Street",
      "Two bin-day notes",
    ]);
    expect(a4.letGoMore).toBe(1);
  });

  it("says how many more are in the app, and drops forgotten lines first", async () => {
    const page = await stress();
    page.needsYou[0] = { ...(page.needsYou[0] as PageView["needsYou"][number]), forgotten: true };
    const fitted = fit(page, "a4");
    expect(fitted.needsYou.map((l) => l.id)).not.toContain("stress-need-0");
    render(<Sheet page={page} size="a4" fitted={fitted} printedAt="07:00" sentNothing={false} />);
    expect(screen.getByText(`and ${fitted.needsYouMore} more in the app`)).toBeInTheDocument();
    expect(screen.getByText(`and ${fitted.changedMore} more in the app`)).toBeInTheDocument();
  });
});

describe("the printed sheet", () => {
  it("receipts_are_bracketed", async () => {
    const page = await stress();
    render(
      <Sheet
        page={page}
        size="a4"
        fitted={fit(page, "a4")}
        printedAt="07:00"
        sentNothing={false}
      />,
    );
    const first = within(sheet()).getAllByText(/\[Letter · /);
    expect(first.length).toBeGreaterThan(0);
    for (const chip of first) expect(chip.textContent?.trim()).toMatch(/^\[.+\]$/);
    expect(within(sheet()).getAllByText(/\[.+\]/).length).toBeGreaterThanOrEqual(12);
  });

  it("prints a which-is-right line with its choices as tick circles and their sources", async () => {
    const page = await stress();
    render(
      <Sheet
        page={page}
        size="a4"
        fitted={fit(page, "a4")}
        printedAt="07:00"
        sentNothing={false}
      />,
    );
    expect(screen.getByText("Dentist appointment: which is right?")).toBeInTheDocument();
    const choice = screen.getByText(/Thu 20 Nov/).closest("li") as HTMLElement;
    expect(choice).toHaveTextContent(/\[.+\]/);
  });

  it("asks the question with something to tick", async () => {
    const page = await stress();
    render(
      <Sheet
        page={page}
        size="a4"
        fitted={fit(page, "a4")}
        printedAt="07:00"
        sentNothing={false}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "One question · tick, then tell Nocturne later" }),
    ).toBeInTheDocument();
    for (const answer of ["Yes", "No", "Not sure"])
      expect(screen.getByText(answer)).toBeInTheDocument();
  });

  it("footer_lines", async () => {
    const page = await stress();
    const draw = (privateSkipped: number, sentNothing: boolean) => {
      cleanup();
      render(
        <Sheet
          page={{ ...page, privateSkipped }}
          size="a4"
          fitted={fit(page, "a4")}
          printedAt="07:00"
          sentNothing={sentNothing}
        />,
      );
      return within(sheet()).getByRole("contentinfo");
    };
    expect(draw(2, true)).toHaveTextContent(
      "Printed 07:00 on this computer. Private items left out.Sent to the internet: nothing",
    );
    expect(draw(0, false)).toHaveTextContent(/^Printed 07:00 on this computer\.$/);
    expect(draw(0, false)).not.toHaveTextContent("Sent to the internet");
  });

  it("uses the paper rule for each size, a receipt as long as it measures", () => {
    expect(pageRule("a4")).toContain("size: A4");
    expect(pageRule("letter")).toContain("size: letter");
    expect(pageRule("receipt", 143.2)).toContain("size: 80mm 146mm");
    expect(pageRule("receipt", 143.2)).not.toContain("500");
    expect(pixelsToMm(96)).toBeCloseTo(25.4);
  });
});

describe("the print screen", () => {
  const open = async (date = PRINT_STRESS_DATE) => {
    const platform = fakePlatform();
    const engine = createMockEngine();
    renderApp({ hash: `#/print/${date}`, engine, platform });
    await screen.findByRole("article", { name: "Print" });
    return { platform, engine };
  };

  it("draws no shell around the sheet and starts on A4", async () => {
    await open();
    expect(screen.queryByRole("navigation", { name: "Main" })).toBeNull();
    expect(screen.getByRole("radio", { name: "A4" })).toBeChecked();
    expect(sheet()).toHaveAttribute("data-size", "a4");
    expect(document.querySelector("style")?.textContent).toContain("size: A4");
  });

  it("leaves private lines out of what it prints", async () => {
    const engine = createMockEngine();
    const page = await engine.getPage("2026-11-13");
    const line = page.needsYou.find((l) => l.memoryIds.length > 0);
    expect(line).toBeDefined();
    const words = (line?.text ?? "").replace(/\*\*/g, "");
    renderApp({ hash: "#/print/2026-11-13", engine });
    await screen.findByRole("article", { name: "Print" });
    // The line is there to begin with, so its absence below means something.
    expect(sheet().textContent).toContain(words);
    cleanup();
    await engine.setPrivate(line?.memoryIds[0] as string, true);
    renderApp({ hash: "#/print/2026-11-13", engine });
    await screen.findByRole("article", { name: "Print" });
    expect(sheet().textContent).not.toContain(words);
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Private items left out.");
    expect(
      screen.getByRole("heading", { level: 2, name: "Friday 13 November" }),
    ).toBeInTheDocument();
  });

  it("remembers the paper for next time", async () => {
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Letter" }));
    expect(sheet()).toHaveAttribute("data-size", "letter");
    expect(window.localStorage.getItem("nocturne-print-size")).toBe("letter");
    cleanup();
    await open();
    expect(screen.getByRole("radio", { name: "Letter" })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: /Receipt/ }));
    expect(sheet()).toHaveAttribute("data-size", "receipt");
  });

  it("still works when the browser will not store anything", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    await open();
    await userEvent.click(screen.getByRole("radio", { name: "Letter" }));
    expect(sheet()).toHaveAttribute("data-size", "letter");
  });

  it("opens the system's print dialog, and says so when it cannot", async () => {
    const { platform } = await open();
    await userEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(platform.printed()).toBe(1);
    platform.print = vi.fn().mockRejectedValue(new Error("no"));
    await userEvent.click(screen.getByRole("button", { name: "Print" }));
    expect(await screen.findByText("I couldn’t open printing. Nothing was lost.")).toBeVisible();
  });

  it("says nothing was sent to the internet only when that was measured", async () => {
    const engine = createMockEngine();
    const real = await engine.getPrivacy();
    vi.spyOn(engine, "getPrivacy").mockResolvedValue({
      ...real,
      sentMeasured: true,
      sentToInternetBytes: 0,
    });
    renderApp({ hash: `#/print/${PRINT_STRESS_DATE}`, engine });
    await screen.findByRole("article", { name: "Print" });
    await waitFor(() =>
      expect(screen.getByRole("contentinfo")).toHaveTextContent("Sent to the internet: nothing"),
    );
    cleanup();
    vi.spyOn(engine, "getPrivacy").mockResolvedValue({
      ...real,
      sentMeasured: false,
      sentToInternetBytes: 0,
    });
    renderApp({ hash: `#/print/${PRINT_STRESS_DATE}`, engine });
    await screen.findByRole("article", { name: "Print" });
    expect(screen.getByRole("contentinfo")).not.toHaveTextContent("Sent to the internet");
  });

  it("says a missing page in words, with nothing to retry", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "getEdition").mockRejectedValue(new EngineError("notFound", "page"));
    renderApp({ hash: "#/print/2026-01-02", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("There is no page for that day.");
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("explains a page that could not be opened", async () => {
    const engine = createMockEngine();
    vi.spyOn(engine, "getEdition").mockRejectedValue(new Error("no"));
    renderApp({ hash: "#/print/2026-11-13", engine });
    expect(await screen.findByRole("alert")).toHaveTextContent("I couldn’t get that page ready");
  });
});
