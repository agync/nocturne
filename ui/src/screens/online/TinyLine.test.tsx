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
  "Later, an online service you choose can write your answers. Nothing is sent until you turn it on.";
const WHERE = "Find it in Settings, under Online answers.";

const openChoosing = async (scenario: string) => {
  setViewport(1280);
  window.history.replaceState({}, "", `/?mockSetup=${scenario}`);
  const engine = createMockEngine();
  window.history.replaceState({}, "", "/");
  renderApp({ hash: "#/setup?step=choosing", engine });
  await screen.findByRole("group", { name: "How much to set up" });
};

const where = () => screen.queryByText((_, el) => el?.tagName === "P" && el.textContent === WHERE);

describe("setup.tiny_line", () => {
  it("is shown when the lightest mind is the one chosen", async () => {
    await openChoosing("tiny");
    expect(screen.getByText(LINE)).toBeInTheDocument();
    expect(where()).toBeInTheDocument();
  });

  it("is not shown for a bigger mind, and appears when Tiny is picked", async () => {
    await openChoosing("large");
    expect(screen.queryByText(LINE)).toBeNull();
    expect(where()).toBeNull();
    await userEvent.click(screen.getByRole("radio", { name: /Tiny/ }));
    expect(screen.getByText(LINE)).toBeInTheDocument();
    expect(where()).toBeInTheDocument();
  });

  it("names Settings and Online answers with chips that are plain text", async () => {
    await openChoosing("tiny");
    const line = where() as HTMLElement;
    const chips = [...line.querySelectorAll("[data-chip]")];
    expect(chips.map((c) => c.textContent)).toEqual(["Settings", "Online answers"]);
    expect(chips[0]?.querySelector("svg")).toHaveAttribute("data-icon", "settings");
    expect(chips[1]?.querySelector("svg")).toBeNull();
    expect(line.querySelector("button, a, [tabindex], [role]")).toBeNull();
  });
});
