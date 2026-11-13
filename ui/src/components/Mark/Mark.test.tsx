import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { copyDeck } from "../../copy/t";
import { Mark, type MarkState, markStateFor } from "./Mark";

const STATES: MarkState[] = ["resting", "reading", "tidying", "needsYou", "paused"];

describe("Mark", () => {
  it("renders the five states, each named from the copy deck", () => {
    for (const state of STATES) {
      const { unmount } = render(<Mark state={state} />);
      expect(screen.getByRole("img", { name: copyDeck[`mark.${state}`] })).toHaveAttribute(
        "data-state",
        state,
      );
      unmount();
    }
  });

  it("keeps quiet when words sit beside it", () => {
    render(<Mark state="resting" decorative />);
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("sizes itself", () => {
    const { container } = render(<Mark state="resting" size={12} />);
    expect(container.querySelector("svg")).toHaveAttribute("width", "12");
  });

  it("maps engine states to the drawn states (B9)", () => {
    expect(markStateFor("starting")).toBe("reading");
    expect(markStateFor("gettingReady")).toBe("reading");
    expect(markStateFor("reading")).toBe("reading");
    expect(markStateFor("resting")).toBe("resting");
    expect(markStateFor("stopped")).toBe("resting");
    expect(markStateFor("tidying")).toBe("tidying");
    expect(markStateFor("needsYou")).toBe("needsYou");
    expect(markStateFor("paused")).toBe("paused");
  });
});
