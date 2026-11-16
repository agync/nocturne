import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { t } from "../../copy/t";
import { ControlChip } from "./ControlChip";
import { CHIP_IDS, CHIPS } from "./registry";

describe("ControlChip", () => {
  it("shows the exact words of the control's own copy key", () => {
    for (const id of CHIP_IDS) {
      const { container, unmount } = render(<ControlChip id={id} />);
      expect(container.textContent, id).toBe(t(CHIPS[id].copy));
      unmount();
    }
  });

  it("draws the gear for Settings, hidden from assistive technology", () => {
    const { container } = render(<ControlChip id="settings" />);
    const icon = container.querySelector("svg");
    expect(icon).toHaveAttribute("data-icon", "settings");
    expect(icon).toHaveAttribute("aria-hidden", "true");
    expect(icon).toHaveAttribute("width", "14");
  });

  it("draws no icon for a control that has none", () => {
    const { container } = render(<ControlChip id="privacy" />);
    expect(container.querySelector("svg")).toBeNull();
  });

  it("is plain text: no role, not focusable, nothing to press", () => {
    const { container } = render(<ControlChip id="settings" />);
    const chip = container.firstElementChild as HTMLElement;
    expect(chip.tagName).toBe("SPAN");
    expect(chip).not.toHaveAttribute("role");
    expect(chip).not.toHaveAttribute("tabindex");
    expect(chip).not.toHaveAttribute("aria-label");
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("is found by its words and named by them alone", () => {
    render(<ControlChip id="settings" />);
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });
});

describe("the chip registry", () => {
  it("names only controls whose copy key is in the deck", () => {
    for (const id of CHIP_IDS) {
      expect(t(CHIPS[id].copy), id).not.toBe("");
    }
  });
});
