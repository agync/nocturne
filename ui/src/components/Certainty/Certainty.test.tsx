import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Certainty } from "./Certainty";

const filledBars = (container: HTMLElement) =>
  container.querySelectorAll('[data-filled="true"]').length;

describe("Certainty", () => {
  it("shows two of three bars and the words for fairly sure", () => {
    const { container } = render(<Certainty level="fairlySure" />);
    expect(filledBars(container)).toBe(2);
    expect(container.querySelectorAll("[data-filled]")).toHaveLength(3);
    expect(screen.getByText("Fairly sure")).toBeInTheDocument();
  });

  it("fills three for sure and one for not sure", () => {
    const sure = render(<Certainty level="sure" />);
    expect(filledBars(sure.container)).toBe(3);
    const notSure = render(<Certainty level="notSure" />);
    expect(filledBars(notSure.container)).toBe(1);
    expect(screen.getByText("Not sure")).toBeInTheDocument();
  });

  it("never shows a percentage", () => {
    const { container } = render(<Certainty level="sure" showHint />);
    expect(container.textContent).not.toMatch(/%/);
    expect(container.textContent).toContain("confirmed by you or two sources");
  });

  it("hides the decorative bars from screen readers", () => {
    const { container } = render(<Certainty level="sure" />);
    expect(container.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });
});
