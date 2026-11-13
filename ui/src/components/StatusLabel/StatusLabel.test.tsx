import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ItemStatus } from "../../engine/bindings";
import { StatusLabel } from "./StatusLabel";

const ITEM: Array<[ItemStatus, string]> = [
  ["kept", "Kept"],
  ["reading", "Reading…"],
  ["understood", "Understood"],
  ["partlyUnderstood", "Partly understood"],
  ["couldntRead", "Couldn’t read"],
  ["needsYou", "Needs you"],
  ["unconfirmed", "Unconfirmed"],
];

describe("StatusLabel", () => {
  it("says the status in words for every item status", () => {
    for (const [status, words] of ITEM) {
      const { unmount } = render(<StatusLabel status={status} />);
      expect(screen.getByText(words)).toHaveAttribute("data-status", status);
      unmount();
    }
  });

  it("also covers the memory flags", () => {
    render(
      <>
        <StatusLabel status="fading" />
        <StatusLabel status="letGo" />
        <StatusLabel status="private" />
        <StatusLabel status="keepForever" />
      </>,
    );
    for (const words of ["Fading", "Let go", "Private", "Kept forever"]) {
      expect(screen.getByText(words)).toBeInTheDocument();
    }
  });

  it("says when a kept item will be read", () => {
    render(<StatusLabel status="kept" tonight />);
    expect(screen.getByText("Kept · reads tonight")).toBeInTheDocument();
  });

  it("offers a retry beside Couldn’t read", async () => {
    const onRetry = vi.fn();
    render(<StatusLabel status="couldntRead" onRetry={onRetry} />);
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(screen.getByText("Couldn’t read")).toBeInTheDocument();
  });
});
