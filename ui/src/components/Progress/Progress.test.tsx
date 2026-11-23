import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Progress } from "./Progress";

describe("Progress", () => {
  it("always renders its words", () => {
    render(
      <Progress value={4.1} max={11} label="Getting ready" detail="4.1 of 11 GB · about 18 min" />,
    );
    expect(screen.getByText("Getting ready")).toBeVisible();
    expect(screen.getByText("4.1 of 11 GB · about 18 min")).toBeVisible();
  });

  it("exposes the words and the position to screen readers", () => {
    render(<Progress value={4} max={8} label="Getting ready" detail="4 of 8 GB" />);
    const bar = screen.getByRole("progressbar", { name: "Getting ready" });
    expect(bar).toHaveAttribute("aria-valuenow", "4");
    expect(bar).toHaveAttribute("aria-valuemax", "8");
    expect(bar).toHaveAttribute("aria-valuetext", "Getting ready, 4 of 8 GB");
  });

  it("says Paused when paused and draws the fill differently", () => {
    const { container } = render(
      <Progress value={1} max={2} label="Paused · resumes on Wi-Fi" detail="4.1 of 11 GB" paused />,
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuetext",
      "Paused · resumes on Wi-Fi, Paused, 4.1 of 11 GB",
    );
    expect(container.firstElementChild).toHaveAttribute("data-paused", "true");
  });

  it("clamps a value outside the range", () => {
    render(<Progress value={20} max={10} label="Reading" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "10");
  });

  it("shows no amount while the amount is not known", () => {
    const { container } = render(<Progress value={null} max={1} label="Getting it ready" />);
    const bar = screen.getByRole("progressbar");
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).not.toHaveAttribute("aria-valuemax");
    expect(bar).toHaveAttribute("aria-valuetext", "Getting it ready");
    expect(container.firstElementChild).toHaveAttribute("data-unknown", "true");
  });
});
