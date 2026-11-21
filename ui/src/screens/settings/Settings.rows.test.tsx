import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

describe("settings", () => {
  it("settings.rows_link_when_present", async () => {
    renderApp({ hash: "#/settings", engine: createMockEngine() });
    await screen.findByRole("combobox", { name: "Voice, name" });
    expect(await screen.findByRole("link", { name: "Online answers, Off. Open" })).toHaveAttribute(
      "href",
      "#/settings/online",
    );
    const models = await screen.findByRole("link", {
      name: "Choose what runs each job, Standard, for this computer. Open",
    });
    expect(models).toHaveAttribute("href", "#/settings/models");
    expect(models).toHaveAccessibleDescription(
      "What answers your questions, reads recordings, reads aloud and finds memories",
    );
    expect(screen.queryByText(/\bModels?\b/)).toBeNull();
    window.location.hash = "#/settings/advanced";
    await screen.findByRole("heading", { level: 1, name: "Advanced details" });
    expect(await screen.findByRole("link", { name: "Choose for each job…" })).toHaveAttribute(
      "href",
      "#/settings/models",
    );
  });

  it("shows From your phones only once a phone with the app is paired", async () => {
    const { unmount } = renderApp({ hash: "#/settings", engine: createMockEngine() });
    await screen.findByRole("combobox", { name: "Voice, name" });
    expect(screen.queryByRole("link", { name: "From your phones, open" })).toBeNull();
    unmount();
    renderApp({
      hash: "#/settings",
      engine: withPhones(createMockEngine(), "2026-11-13T07:02:00Z"),
    });
    const row = await screen.findByRole("link", { name: "From your phones, open" });
    expect(row).toHaveAttribute("href", "#/settings/phones");
    expect(row).toHaveAccessibleDescription("Calendar and health your phones bring");
  });

  it("leaves From your phones out when the only phone is in the browser", async () => {
    const engine = withPhones(createMockEngine(), "2026-11-13T07:02:00Z");
    engine.sim.fixtures.phones = engine.sim.fixtures.phones.filter((p) => p.inBrowser);
    renderApp({ hash: "#/settings", engine });
    await screen.findByRole("combobox", { name: "Voice, name" });
    expect(screen.queryByRole("link", { name: "From your phones, open" })).toBeNull();
  });

  it("ends with the Nocturne guide, which opens the reader", async () => {
    renderApp({ hash: "#/settings", engine: createMockEngine() });
    const row = await screen.findByRole("link", { name: "Open the Nocturne guide, Read" });
    expect(row).toHaveAttribute("href", "#/guide?from=settings");
    expect(row).toHaveTextContent("Read ›");
    expect(row).toHaveAccessibleDescription("How to do things in Nocturne");
    const rows = screen.getAllByRole("listitem").filter((li) => li.closest("ul"));
    expect(rows.at(-1)).toContainElement(row);
  });
});
