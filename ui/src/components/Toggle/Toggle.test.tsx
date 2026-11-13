import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Toggle } from "./Toggle";

describe("Toggle", () => {
  it("is a switch named by its label, with the state in words", () => {
    render(
      <Toggle
        checked
        label="Start with this computer"
        hint="Needed for the overnight tidy"
        onChange={() => undefined}
      />,
    );
    const sw = screen.getByRole("switch", { name: "Start with this computer" });
    expect(sw).toBeChecked();
    expect(sw).toHaveAccessibleDescription("Needed for the overnight tidy");
    expect(screen.getByText("On")).toBeInTheDocument();
  });

  it("flips with Space, Enter and a click on the label", async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} label="Print the page" onChange={onChange} />);
    expect(screen.getByText("Off")).toBeInTheDocument();
    await userEvent.tab();
    await userEvent.keyboard(" ");
    await userEvent.keyboard("{Enter}");
    await userEvent.click(screen.getByText("Print the page"));
    expect(onChange).toHaveBeenCalledTimes(3);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("does nothing when disabled", async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} disabled label="Lock" onChange={onChange} />);
    await userEvent.click(screen.getByRole("switch"));
    expect(onChange).not.toHaveBeenCalled();
  });
});
