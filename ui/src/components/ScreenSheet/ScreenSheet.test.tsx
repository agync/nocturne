import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ScreenSheet } from "./ScreenSheet";

describe("ScreenSheet", () => {
  it("wraps its children in a padded sheet by default", () => {
    render(
      <ScreenSheet>
        <h1>Settings</h1>
      </ScreenSheet>,
    );
    const sheet = screen.getByRole("heading", { name: "Settings" }).parentElement;
    expect(sheet).toHaveAttribute("data-flush", "false");
  });

  it("drops the padding when flush", () => {
    render(
      <ScreenSheet flush>
        <p>pane</p>
      </ScreenSheet>,
    );
    expect(screen.getByText("pane").parentElement).toHaveAttribute("data-flush", "true");
  });

  it("adds a caller class beside its own", () => {
    render(
      <ScreenSheet className="extra">
        <p>x</p>
      </ScreenSheet>,
    );
    expect(screen.getByText("x").parentElement).toHaveClass("extra");
  });
});
