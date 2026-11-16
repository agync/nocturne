import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KeyCap, KeyCaps } from "./KeyCap";

describe("KeyCap", () => {
  it("is a key drawn in the text, named by the key it shows", () => {
    render(
      <p>
        Press <KeyCap>/</KeyCap> to come back here.
      </p>,
    );
    expect(screen.getByText("/")).toBeInTheDocument();
    expect(screen.getByText("/").tagName).toBe("KBD");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("reads several keys as the words with a space between", () => {
    const { container } = render(
      <p>
        <KeyCaps keys={["Ctrl", "Shift", "Space"]} />
      </p>,
    );
    expect(container.textContent).toBe("Ctrl Shift Space");
    expect(container.querySelectorAll("kbd")).toHaveLength(3);
  });
});
