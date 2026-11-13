import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type RefObject, useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog";

function Harness({ initial }: { initial?: boolean }) {
  const [open, setOpen] = useState(false);
  const keep = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Forget…
      </button>
      {open ? (
        <Dialog
          title="Forget 3 things?"
          onClose={() => setOpen(false)}
          initialFocus={initial ? (keep as RefObject<HTMLElement | null>) : undefined}
        >
          <button type="button">Forget</button>
          <button type="button" ref={keep}>
            Keep them
          </button>
        </Dialog>
      ) : null}
    </>
  );
}

describe("Dialog", () => {
  it("is a named modal dialog", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Forget…" }));
    const dialog = screen.getByRole("dialog", { name: "Forget 3 things?" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
  });

  it("puts focus where the caller says, so Enter cannot forget by accident", async () => {
    render(<Harness initial />);
    await userEvent.click(screen.getByRole("button", { name: "Forget…" }));
    expect(screen.getByRole("button", { name: "Keep them" })).toHaveFocus();
  });

  it("traps focus: Tab and Shift+Tab cycle inside", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Forget…" }));
    const close = screen.getByRole("button", { name: "Close" });
    const forget = screen.getByRole("button", { name: "Forget" });
    const keep = screen.getByRole("button", { name: "Keep them" });
    expect(close).toHaveFocus();
    await userEvent.tab();
    expect(forget).toHaveFocus();
    await userEvent.tab();
    expect(keep).toHaveFocus();
    await userEvent.tab();
    expect(close).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(keep).toHaveFocus();
  });

  it("closes on Escape and returns focus to the opener", async () => {
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Forget…" });
    await userEvent.click(opener);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(opener).toHaveFocus();
  });

  it("closes from its own Close button", async () => {
    const onClose = vi.fn();
    render(
      <Dialog title="Menu" onClose={onClose}>
        <p>Hello</p>
      </Dialog>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("handles a dialog opened from a dialog: only the inner one hears Escape, and inert is restored", async () => {
    const root = document.createElement("div");
    root.id = "root";
    document.body.appendChild(root);
    function Nested() {
      const [outer, setOuter] = useState(true);
      const [inner, setInner] = useState(false);
      return (
        <>
          {outer ? (
            <Dialog title="Outer" onClose={() => setOuter(false)}>
              <button type="button" onClick={() => setInner(true)}>
                Open inner
              </button>
              {inner ? (
                <Dialog title="Inner" onClose={() => setInner(false)}>
                  <p>Inside</p>
                </Dialog>
              ) : null}
            </Dialog>
          ) : null}
        </>
      );
    }
    render(<Nested />, { container: root.appendChild(document.createElement("div")) });
    expect(root).toHaveAttribute("inert");
    await userEvent.click(screen.getByRole("button", { name: "Open inner" }));
    const outerPanel = screen.getByRole("dialog", { name: "Outer", hidden: true });
    expect(outerPanel.closest('[role="presentation"]')).toHaveAttribute("inert");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Inner" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Outer" })).toBeInTheDocument();
    expect(
      screen.getByRole("dialog", { name: "Outer" }).closest('[role="presentation"]'),
    ).not.toHaveAttribute("inert");
    expect(root).toHaveAttribute("inert");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(root).not.toHaveAttribute("inert");
    root.remove();
  });

  it("ignores Escape, the backdrop and Close while locked", async () => {
    const onClose = vi.fn();
    render(
      <Dialog title="Working" locked onClose={onClose}>
        <button type="button">Inside</button>
      </Dialog>,
    );
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    const backdrop = screen.getByRole("dialog").parentElement as HTMLElement;
    await userEvent.click(backdrop);
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
  });

  it("closes on Escape again once it is unlocked", async () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <Dialog title="Working" locked onClose={onClose}>
        <button type="button">Inside</button>
      </Dialog>,
    );
    rerender(
      <Dialog title="Working" onClose={onClose}>
        <button type="button">Inside</button>
      </Dialog>,
    );
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });
});
