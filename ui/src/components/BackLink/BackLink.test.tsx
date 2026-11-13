import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BackLink } from "./BackLink";

describe("BackLink", () => {
  it("links to the hash route and names the destination", () => {
    render(<BackLink to="/settings" label="Settings" />);
    const link = screen.getByRole("link", { name: "Settings" });
    expect(link).toHaveAttribute("href", "#/settings");
    expect(link).toHaveClass("hit");
  });

  it("hides the arrow from screen readers", () => {
    const { container } = render(<BackLink to="/today" label="Today" />);
    const arrow = container.querySelector("span");
    expect(arrow).toHaveAttribute("aria-hidden", "true");
    expect(arrow).toHaveTextContent("←");
  });
});
