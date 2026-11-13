import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

afterEach(() => {
  window.location.hash = "";
  window.history.replaceState({}, "", "/");
});

const LINE =
  "On a small computer, you can later choose to have answers written online. See Settings.";

const openChoosing = async (scenario: string) => {
  setViewport(1280);
  window.history.replaceState({}, "", `/?mockSetup=${scenario}`);
  const engine = createMockEngine();
  window.history.replaceState({}, "", "/");
  renderApp({ hash: "#/setup?step=choosing", engine });
  await screen.findByRole("group", { name: "How much to set up" });
};

describe("setup.tiny_line", () => {
  it("is shown when the lightest mind is the one chosen", async () => {
    await openChoosing("tiny");
    expect(screen.getByText(LINE)).toBeInTheDocument();
  });

  it("is not shown for a bigger mind, and appears when Tiny is picked", async () => {
    await openChoosing("large");
    expect(screen.queryByText(LINE)).toBeNull();
    await userEvent.click(screen.getByRole("radio", { name: /Tiny/ }));
    expect(screen.getByText(LINE)).toBeInTheDocument();
  });
});
