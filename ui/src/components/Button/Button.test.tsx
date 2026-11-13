import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("is a real button that does not submit by accident", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Bring back</Button>);
    const button = screen.getByRole("button", { name: "Bring back" });
    expect(button).toHaveAttribute("type", "button");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("fills only the primary variant at rest", () => {
    const { rerender } = render(<Button variant="primary">Keep</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-filled", "true");
    for (const variant of ["secondary", "quiet", "forget"] as const) {
      rerender(<Button variant={variant}>x</Button>);
      expect(screen.getByRole("button")).toHaveAttribute("data-filled", "false");
    }
  });

  it("never fills Forget until the confirm step", () => {
    const { rerender } = render(<Button variant="forget">Forget</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-filled", "false");
    expect(screen.getByRole("button")).toHaveAttribute("data-state", "rest");
    rerender(
      <Button variant="forget" confirming>
        Forget
      </Button>,
    );
    expect(screen.getByRole("button")).toHaveAttribute("data-filled", "true");
    expect(screen.getByRole("button")).toHaveAttribute("data-state", "confirming");
  });

  it("does not act when off", async () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Keep
      </Button>,
    );
    await userEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("offers a 44 px touch size", () => {
    render(<Button touch>Keep</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-touch", "true");
  });

  it("marks the flush variant", () => {
    const { rerender } = render(<Button variant="quiet">Undo</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-flush", "false");
    rerender(
      <Button variant="quiet" flush>
        Undo
      </Button>,
    );
    expect(screen.getByRole("button")).toHaveAttribute("data-flush", "true");
  });
});
