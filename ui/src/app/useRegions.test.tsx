import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import { describe, expect, it } from "vitest";
import { useRegionCycling } from "./useRegions";

function Fixture({ withToast }: { withToast: boolean }) {
  const root = useRef<HTMLDivElement>(null);
  useRegionCycling(root);
  return (
    <>
      <div ref={root}>
        <div data-region="keep">
          <textarea aria-label="bar" />
        </div>
        <main data-region="main" tabIndex={-1} aria-label="page">
          <button type="button">inside page</button>
        </main>
        <nav data-region="rail">
          <a href="#/today">Today</a>
        </nav>
      </div>
      {withToast ? (
        <div data-region="toast">
          <button type="button">Try again</button>
        </div>
      ) : null}
    </>
  );
}

describe("F6", () => {
  it("includes a toast with an action in the cycle, after the rail", async () => {
    render(<Fixture withToast />);
    screen.getByLabelText("bar").focus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByRole("main")).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByRole("link", { name: "Today" })).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByRole("button", { name: "Try again" })).toHaveFocus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByLabelText("bar")).toHaveFocus();
  });

  it("skips the toast when there is none", async () => {
    render(<Fixture withToast={false} />);
    screen.getByRole("link", { name: "Today" }).focus();
    await userEvent.keyboard("{F6}");
    expect(screen.getByLabelText("bar")).toHaveFocus();
  });
});
