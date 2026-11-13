import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

describe("models page with the online mock", () => {
  it("reads the online state a bounded number of times and stays quiet", async () => {
    setViewport(1280);
    const engine = createMockEngine();
    const read = vi.spyOn(engine, "getOnline");
    const models = vi.spyOn(engine, "getModels");
    renderApp({ hash: "#/settings/models", engine });
    await screen.findByRole("link", { name: /Change Answering questions/ });
    await new Promise((r) => setTimeout(r, 400));
    expect(read.mock.calls.length).toBeLessThanOrEqual(2);
    expect(models.mock.calls.length).toBeLessThanOrEqual(3);
    const settled = read.mock.calls.length;
    await new Promise((r) => setTimeout(r, 400));
    expect(read.mock.calls.length).toBe(settled);
  });
});
