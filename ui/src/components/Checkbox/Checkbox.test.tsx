import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("is a checkbox named by its label, with its hint as the description", () => {
    render(
      <Checkbox
        checked
        label="Delete the originals too"
        hint="The photo and the voice note"
        onChange={vi.fn()}
      />,
    );
    const box = screen.getByRole("checkbox", { name: "Delete the originals too" });
    expect(box).toBeChecked();
    expect(box).toHaveAccessibleDescription("The photo and the voice note");
  });

  it("flips with Space and with a click on the words", async () => {
    const onChange = vi.fn();
    render(<Checkbox checked={false} label="Include originals" onChange={onChange} />);
    await userEvent.tab();
    await userEvent.keyboard(" ");
    await userEvent.click(screen.getByText("Include originals"));
    expect(onChange).toHaveBeenNthCalledWith(1, true);
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it("says mixed for a select-all over a partly chosen set", () => {
    render(<Checkbox checked={false} indeterminate label="All" onChange={vi.fn()} />);
    expect(screen.getByRole("checkbox")).toBePartiallyChecked();
  });

  it("does nothing when disabled", async () => {
    const onChange = vi.fn();
    render(<Checkbox checked={false} disabled label="Locked" onChange={onChange} />);
    await userEvent.click(screen.getByRole("checkbox"));
    expect(onChange).not.toHaveBeenCalled();
  });
});
