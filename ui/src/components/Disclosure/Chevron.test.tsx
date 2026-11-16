import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Chevron } from "./Chevron";

describe("Chevron", () => {
  it("sits before the words and leaves the button's name as the words alone", () => {
    render(
      <button type="button" aria-expanded="false">
        <Chevron />
        Why this one?
      </button>,
    );
    const button = screen.getByRole("button", { name: "Why this one?" });
    expect(button.firstElementChild).toHaveAttribute("data-chevron");
    expect(button.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  });
});
