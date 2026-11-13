import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ICON_NAMES, Icon } from "./Icon";

describe("Icon", () => {
  it("offers exactly the twenty icons of the design", () => {
    expect(ICON_NAMES.sort()).toEqual(
      [
        "mic",
        "play",
        "pause",
        "skip-forward",
        "printer",
        "search",
        "sun",
        "moon",
        "folder-inbox",
        "image",
        "file-text",
        "audio-lines",
        "lock",
        "eye-off",
        "rotate-ccw",
        "trash",
        "settings",
        "x",
        "chevron-down",
        "paperclip",
      ].sort(),
    );
  });

  it("is hidden from screen readers unless it has a label", () => {
    const { container, rerender } = render(<Icon name="mic" />);
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    rerender(<Icon name="mic" label="Microphone" />);
    expect(container.querySelector("svg")).toHaveAttribute("aria-label", "Microphone");
  });

  it("draws every icon at 20 px with a 1.5 px rendered stroke by default", () => {
    for (const name of ICON_NAMES) {
      const { container, unmount } = render(<Icon name={name} />);
      const svg = container.querySelector("svg");
      expect(svg, name).toHaveAttribute("width", "20");
      // absolute stroke: 1.5 px on screen means 1.8 units on lucide’s 24 unit grid at 20 px
      expect(Number(svg?.getAttribute("stroke-width")), name).toBeCloseTo(1.8, 5);
      unmount();
    }
  });
});
