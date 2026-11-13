import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { onlineReads } from "../online/useOnline";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  onlineReads.on = false;
});

describe("settings", () => {
  it("settings.online_row_hidden_without_a_view", async () => {
    renderApp({ hash: "#/settings", engine: createMockEngine() });
    await screen.findByRole("combobox", { name: "Voice, name" });
    expect(screen.queryByText("Online answers")).toBeNull();
  });

  it("settings.rows_link_when_present", async () => {
    onlineReads.on = true;
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
});
