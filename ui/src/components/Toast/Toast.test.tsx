import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider, useToast } from "./Toast";

function Opener({ onAction }: { onAction?: () => void }) {
  const { show } = useToast();
  return (
    <button
      type="button"
      onClick={() =>
        show({
          lead: "Kept.",
          message: "I’ll read it tonight.",
          action: onAction ? { label: "Undo", onAction } : undefined,
        })
      }
    >
      Keep
    </button>
  );
}

describe("Toast", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("announces politely and shows the lead and the message", async () => {
    render(
      <ToastProvider>
        <Opener />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    const region = screen.getByRole("status");
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(region).toHaveTextContent("Kept.I’ll read it tonight.");
  });

  it("goes away by itself", async () => {
    render(
      <ToastProvider>
        <Opener />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.getByText("Kept.")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6100);
    });
    expect(screen.queryByText("Kept.")).toBeNull();
  });

  it("runs its action and goes away", async () => {
    const onAction = vi.fn();
    render(
      <ToastProvider>
        <Opener onAction={onAction} />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(onAction).toHaveBeenCalledOnce();
    expect(screen.queryByText("Kept.")).toBeNull();
  });

  it("stays while the pointer is on it", async () => {
    render(
      <ToastProvider>
        <Opener />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    await userEvent.hover(screen.getByText("Kept."));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(screen.getByText("Kept.")).toBeInTheDocument();
  });

  it("keeps a toast with an action for ten seconds, and longer while it has focus", async () => {
    const onAction = vi.fn();
    render(
      <ToastProvider>
        <Opener onAction={onAction} />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(9000);
    });
    expect(screen.getByText("Kept.")).toBeInTheDocument();
    screen.getByRole("button", { name: "Undo" }).focus();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(screen.getByText("Kept.")).toBeInTheDocument();
    screen.getByRole("button", { name: "Keep" }).focus();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_100);
    });
    expect(screen.queryByText("Kept.")).toBeNull();
  });

  it("can be reached with F6-style focus, closed with Escape, and gives focus back", async () => {
    render(
      <ToastProvider>
        <Opener onAction={vi.fn()} />
      </ToastProvider>,
    );
    const opener = screen.getByRole("button", { name: "Keep" });
    await userEvent.click(opener);
    const toast = document.querySelector('[data-region="toast"]');
    expect(toast).not.toBeNull();
    opener.focus();
    screen.getByRole("button", { name: "Undo" }).focus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByText("Kept.")).toBeNull();
    expect(opener).toHaveFocus();
  });
});
