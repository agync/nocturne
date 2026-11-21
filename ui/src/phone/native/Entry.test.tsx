import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AddressStep, NativeWelcome } from "./Entry";

afterEach(() => cleanup());

const noop = () => undefined;

describe("the app's way in", () => {
  it("says so when the scan read a code that is not from Nocturne", () => {
    render(
      <NativeWelcome
        waiting={0}
        wrongCode
        onScan={noop}
        onType={noop}
        onSite={noop}
        onClear={noop}
      />,
    );
    expect(screen.getByRole("alert").textContent).toBe(
      "That isn’t a code from your computer. Try again.",
    );
  });

  it("shows no message on a fresh Welcome", () => {
    render(<NativeWelcome waiting={0} onScan={noop} onType={noop} onSite={noop} onClear={noop} />);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "Nocturne lives on your computer",
    );
  });

  it("keeps what was typed and says the address did not answer", () => {
    const onNext = vi.fn();
    render(
      <AddressStep
        initial="desk.tail1234.ts.net"
        busy={false}
        error="noAnswer"
        onNext={onNext}
        onScan={noop}
        onBack={noop}
      />,
    );
    expect((screen.getByLabelText("Your computer’s address") as HTMLInputElement).value).toBe(
      "desk.tail1234.ts.net",
    );
    expect(screen.getByRole("alert").textContent).toBe(
      "That address didn’t answer. Check the name, and that Tailscale is on for this phone.",
    );
  });
});
