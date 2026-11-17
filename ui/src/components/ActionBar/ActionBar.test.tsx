import { act, render, renderHook, screen } from "@testing-library/react";
import { StrictMode, useLayoutEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Button } from "../Button/Button";
import { ScreenSheet } from "../ScreenSheet/ScreenSheet";
import { ActionBar } from "./ActionBar";
import { isFlowOpen, useFlowOpen } from "./flow";

function pinBars(height: number) {
  const real = window.getComputedStyle.bind(window);
  vi.spyOn(window, "getComputedStyle").mockImplementation((element, pseudo) =>
    element instanceof HTMLElement && element.hasAttribute("data-action-bar")
      ? ({ position: "sticky" } as CSSStyleDeclaration)
      : real(element, pseudo),
  );
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(height);
}

describe("ActionBar", () => {
  beforeEach(() => vi.restoreAllMocks());
  afterEach(() => vi.restoreAllMocks());

  it("puts the main button first and the line after the buttons", () => {
    render(
      <ActionBar line="Nothing is turned on until the last step.">
        <Button variant="primary">Continue</Button>
        <Button variant="quiet">Not now</Button>
      </ActionBar>,
    );
    const bar = document.querySelector("[data-action-bar]") as HTMLElement;
    const names = Array.from(bar.querySelectorAll("button")).map((b) => b.textContent);
    expect(names).toEqual(["Continue", "Not now"]);
    expect(bar.lastElementChild).toHaveTextContent("Nothing is turned on until the last step.");
  });

  it("lets a disabled button point at the line that says why", () => {
    render(
      <ActionBar lineId="why" line="Continue works once the check passes.">
        <Button variant="primary" disabled aria-describedby="why">
          Continue
        </Button>
      </ActionBar>,
    );
    expect(screen.getByRole("button", { name: "Continue" })).toHaveAccessibleDescription(
      "Continue works once the check passes.",
    );
  });

  it("draws no line when there is none", () => {
    render(
      <ActionBar>
        <Button variant="primary">Turn on</Button>
      </ActionBar>,
    );
    expect(document.querySelector("[data-action-bar] > span")).toBeNull();
  });

  it("comes after the words of its sheet, so they are met first", () => {
    render(
      <ScreenSheet>
        <ActionBar>
          <Button variant="primary">Continue</Button>
        </ActionBar>
        <h1>Words</h1>
      </ScreenSheet>,
    );
    const sheet = screen.getByRole("heading", { name: "Words" }).parentElement as HTMLElement;
    const order = Array.from(sheet.querySelectorAll("h1, button")).map((e) => e.tagName);
    expect(order).toEqual(["H1", "BUTTON"]);
    expect(sheet.lastElementChild).toHaveAttribute("data-action-slot");
  });

  it("says a step flow is open for as long as it is on screen", () => {
    const flow = renderHook(() => useFlowOpen());
    expect(flow.result.current).toBe(false);
    const bar = render(
      <ActionBar>
        <Button variant="primary">Continue</Button>
      </ActionBar>,
    );
    expect(flow.result.current).toBe(true);
    bar.unmount();
    expect(flow.result.current).toBe(false);
  });

  it("keeps room under the words as tall as the pinned bar, and gives it back", () => {
    pinBars(72);
    const view = render(
      <div style={{ overflowY: "auto" }} data-testid="scroller">
        <ScreenSheet>
          <ActionBar>
            <Button variant="primary">Continue</Button>
          </ActionBar>
        </ScreenSheet>
      </div>,
    );
    const scroller = screen.getByTestId("scroller");
    expect(scroller.style.scrollPaddingBottom).toBe("72px");
    view.unmount();
    expect(scroller.style.scrollPaddingBottom).toBe("");
  });

  it("keeps no room when a short window leaves the bar in the page", () => {
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(72);
    render(
      <div style={{ overflowY: "auto" }} data-testid="scroller">
        <ScreenSheet>
          <ActionBar>
            <Button variant="primary">Continue</Button>
          </ActionBar>
        </ScreenSheet>
      </div>,
    );
    expect(screen.getByTestId("scroller").style.scrollPaddingBottom).toBe("");
  });

  it("fades only while words are still hidden, not through the bar and the space before it", () => {
    pinBars(72);
    render(
      <div style={{ overflowY: "auto" }} data-testid="scroller">
        <ScreenSheet>
          <ActionBar>
            <Button variant="primary">Continue</Button>
          </ActionBar>
        </ScreenSheet>
      </div>,
    );
    const scroller = screen.getByTestId("scroller");
    const bar = document.querySelector("[data-action-bar]") as HTMLElement;
    const spacer = bar.previousElementSibling as HTMLElement;
    const place = (wordsEnd: number) =>
      vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (
        this: Element,
      ) {
        const top = this === spacer ? wordsEnd : this === bar ? 528 : 0;
        return {
          top,
          bottom: top,
          left: 0,
          right: 0,
          width: 0,
          height: 0,
          x: 0,
          y: top,
        } as DOMRect;
      });
    // The scroll height still counts the bar and the space, as it does in the real window.
    Object.defineProperty(scroller, "clientHeight", { configurable: true, value: 600 });
    Object.defineProperty(scroller, "scrollHeight", { configurable: true, value: 900 });

    place(700);
    act(() => {
      scroller.dispatchEvent(new Event("scroll"));
    });
    expect(bar).toHaveAttribute("data-more", "true");

    place(520);
    act(() => {
      scroller.dispatchEvent(new Event("scroll"));
    });
    expect(bar).toHaveAttribute("data-more", "false");
  });

  it("counts a flow in the layout phase, before anything is painted", () => {
    const seen: boolean[] = [];
    function Probe() {
      useLayoutEffect(() => {
        seen.push(isFlowOpen());
      }, []);
      return null;
    }
    render(
      <>
        <ActionBar>
          <Button variant="primary">Continue</Button>
        </ActionBar>
        <Probe />
      </>,
    );
    expect(seen).toEqual([true]);
  });

  it("keeps the count balanced under StrictMode, and clears it when the bar leaves", () => {
    const view = render(
      <StrictMode>
        <ActionBar>
          <Button variant="primary">Continue</Button>
        </ActionBar>
      </StrictMode>,
    );
    expect(isFlowOpen()).toBe(true);
    view.rerender(
      <StrictMode>
        <ActionBar>
          <Button variant="primary">Continue</Button>
        </ActionBar>
        <ActionBar>
          <Button variant="primary">Again</Button>
        </ActionBar>
      </StrictMode>,
    );
    view.unmount();
    expect(isFlowOpen()).toBe(false);
  });
});
