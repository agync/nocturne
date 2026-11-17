import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

describe("the guide reader", () => {
  it("opens from Settings at the guide's first page, and goes back to Settings", async () => {
    renderApp({ hash: "#/guide?from=settings", engine: createMockEngine() });
    expect(await screen.findByRole("heading", { level: 1, name: "Using Nocturne" })).toBeVisible();
    expect(document.querySelector('a[href="#/settings"]')).not.toBeNull();
    expect(screen.getAllByRole("link", { name: "Settings" }).length).toBeGreaterThan(0);
    expect(screen.getByText("Nocturne guide", { selector: "p" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "On this page" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "In this guide" })).toBeInTheDocument();
    expect(
      screen.getByText(
        /This guide came with Nocturne .+\. It is on this computer, and reading it sends nothing\./,
      ),
    ).toBeInTheDocument();
  });

  it("opens from an answer at the cited section, marks it, and goes back to Ask", async () => {
    renderApp({
      hash: "#/guide?page=away-from-home&section=add-a-phone&from=ask",
      engine: createMockEngine(),
    });
    const heading = await screen.findByRole("heading", { level: 2, name: "Add a phone" });
    expect(heading.closest("section")).toHaveAttribute("data-marked", "true");
    expect(screen.getByRole("link", { name: "Ask" })).toHaveAttribute("href", "#/ask");
    const chip = heading.closest("section")?.querySelector('[data-chip="addAPhone"]');
    expect(chip).toHaveTextContent("Add a phone");
    expect(screen.getByRole("link", { name: "Away from home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("lists only the pages help may use", async () => {
    renderApp({ hash: "#/guide?from=settings", engine: createMockEngine() });
    await screen.findByRole("heading", { level: 1, name: "Using Nocturne" });
    expect(screen.queryByRole("link", { name: "Faster with the NPU" })).toBeNull();
    expect(screen.queryByRole("link", { name: "Choosing models" })).toBeNull();
    expect(screen.getByRole("link", { name: "Install and remove" })).toBeInTheDocument();
  });
});
