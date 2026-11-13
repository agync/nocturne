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
          message: "Nocturne will read it tonight.",
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
    expect(region).toHaveTextContent("Kept.Nocturne will read it tonight.");
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

  it("shows an identical message once, however often it is shown", async () => {
    function Thrice() {
      const { show } = useToast();
      return (
        <button
          type="button"
          onClick={() => {
            show({ message: "No sound here." });
            show({ message: "No sound here." });
            show({ message: "No sound here." });
          }}
        >
          Three
        </button>
      );
    }
    render(
      <ToastProvider>
        <Thrice />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Three" }));
    expect(screen.getAllByText("No sound here.")).toHaveLength(1);
  });

  it("starts the clock again when the same message is shown again", async () => {
    render(
      <ToastProvider>
        <Opener />
      </ToastProvider>,
    );
    const opener = screen.getByRole("button", { name: "Keep" });
    await userEvent.click(opener);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });
    await userEvent.click(opener);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });
    expect(screen.getAllByText("Kept.")).toHaveLength(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    expect(screen.queryByText("Kept.")).toBeNull();
  });

  it("keeps different messages apart, and an explicit key joins them", async () => {
    function Mixed() {
      const { show } = useToast();
      return (
        <button
          type="button"
          onClick={() => {
            show({ message: "One" });
            show({ message: "Two" });
            show({ message: "Three", key: "same" });
            show({ message: "Four", key: "same" });
          }}
        >
          Mix
        </button>
      );
    }
    render(
      <ToastProvider>
        <Mixed />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Mix" }));
    expect(screen.queryByText("Three")).toBeNull();
    expect(screen.getByText("Four")).toBeInTheDocument();
    expect(screen.getByText("Two")).toBeInTheDocument();
  });

  describe("onExpire", () => {
    function Holder({ onExpire, onAction }: { onExpire: () => void; onAction: () => void }) {
      const { show, dismiss } = useToast();
      return (
        <>
          <button
            type="button"
            onClick={() =>
              show({ message: "Marked as done.", action: { label: "Undo", onAction }, onExpire })
            }
          >
            Open
          </button>
          <button type="button" onClick={() => dismiss(1)}>
            Dismiss
          </button>
        </>
      );
    }
    const setup = async () => {
      const onExpire = vi.fn();
      const onAction = vi.fn();
      render(
        <ToastProvider>
          <Holder onExpire={onExpire} onAction={onAction} />
        </ToastProvider>,
      );
      await userEvent.click(screen.getByRole("button", { name: "Open" }));
      return { onExpire, onAction };
    };

    it("fires once on timeout", async () => {
      const { onExpire, onAction } = await setup();
      expect(onExpire).not.toHaveBeenCalled();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_500);
      });
      expect(onExpire).toHaveBeenCalledTimes(1);
      expect(onAction).not.toHaveBeenCalled();
    });

    it("fires on dismissal", async () => {
      const { onExpire } = await setup();
      await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
      expect(onExpire).toHaveBeenCalledTimes(1);
      expect(screen.queryByText("Marked as done.")).toBeNull();
    });

    it("fires on Escape", async () => {
      const { onExpire } = await setup();
      screen.getByRole("button", { name: "Undo" }).focus();
      await userEvent.keyboard("{Escape}");
      expect(onExpire).toHaveBeenCalledTimes(1);
    });

    it("does not fire when the action is pressed", async () => {
      const { onExpire, onAction } = await setup();
      await userEvent.click(screen.getByRole("button", { name: "Undo" }));
      expect(onAction).toHaveBeenCalledTimes(1);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_500);
      });
      expect(onExpire).not.toHaveBeenCalled();
    });
  });
});
