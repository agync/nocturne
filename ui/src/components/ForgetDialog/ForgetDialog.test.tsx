import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ForgetDialog } from "./ForgetDialog";

describe("ForgetDialog (stub)", () => {
  it("opens a named dialog and closes without forgetting anything", async () => {
    const onClose = vi.fn();
    const onDone = vi.fn();
    render(
      <ForgetDialog
        memoryIds={["a", "b"]}
        itemIds={["i"]}
        defaultIncludeOriginals
        onClose={onClose}
        onDone={onDone}
      />,
    );
    expect(screen.getByRole("dialog", { name: "Forget 2 things?" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep them" }));
    expect(onClose).toHaveBeenCalledOnce();
    expect(onDone).not.toHaveBeenCalled();
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn();
    render(<ForgetDialog memoryIds={["a"]} onClose={onClose} onDone={vi.fn()} />);
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Forget 1 thing?" })).toBeInTheDocument();
  });
});
