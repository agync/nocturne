import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FeelingPoint } from "../../engine/bindings";
import { FeelingField } from "./FeelingField";

afterEach(cleanup);

function Host({
  start = null,
  spy,
}: {
  start?: FeelingPoint | null;
  spy?: (p: FeelingPoint) => void;
}) {
  const [point, setPoint] = useState<FeelingPoint | null>(start);
  return (
    <FeelingField
      point={point}
      onChange={(next) => {
        spy?.(next);
        setPoint(next);
      }}
    />
  );
}

/** jsdom has no layout: give the field a 200 by 200 box at the origin. */
function sized() {
  const field = screen.getByRole("radiogroup", {
    name: "How you feel, by energy and how pleasant it is",
  });
  field.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 200, height: 200, right: 200, bottom: 200, x: 0, y: 0 }) as DOMRect;
  return field;
}

describe("FeelingField", () => {
  it("names the axes outside the field", () => {
    render(<Host />);
    for (const label of ["More energy", "Less energy", "Unpleasant", "Pleasant"]) {
      expect(screen.getByText(new RegExp(label))).toBeInTheDocument();
    }
  });

  it("starts with nothing chosen", () => {
    render(<Host />);
    for (const radio of screen.getAllByRole("radio")) expect(radio).not.toBeChecked();
  });

  it("a press places the point and picks the part it is in", () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    const field = sized();
    // Right and low in the box: pleasant, less energy.
    fireEvent.pointerDown(field, { clientX: 150, clientY: 150, pointerId: 1 });
    expect(spy).toHaveBeenLastCalledWith({ pleasant: 0.5, energy: -0.5 });
    expect(screen.getByRole("radio", { name: "Settled" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Low" })).not.toBeChecked();
  });

  it("dragging moves the point and can cross into another part", () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    const field = sized();
    fireEvent.pointerDown(field, { clientX: 150, clientY: 150, pointerId: 1 });
    fireEvent.pointerMove(field, { clientX: 20, clientY: 20, pointerId: 1 });
    expect(spy).toHaveBeenLastCalledWith({ pleasant: -0.8, energy: 0.8 });
    expect(screen.getByRole("radio", { name: "Wound up" })).toBeChecked();
    fireEvent.pointerUp(field, { pointerId: 1 });
    fireEvent.pointerMove(field, { clientX: 190, clientY: 10, pointerId: 1 });
    expect(spy).toHaveBeenLastCalledWith({ pleasant: -0.8, energy: 0.8 });
  });

  it("keeps the point inside the field", () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    const field = sized();
    fireEvent.pointerDown(field, { clientX: 400, clientY: -50, pointerId: 1 });
    expect(spy).toHaveBeenLastCalledWith({ pleasant: 1, energy: 1 });
  });

  it("arrow keys move the point in small steps and the announcement changes step", async () => {
    render(<Host start={{ pleasant: 0.2, energy: 0.2 }} />);
    const live = screen.getByRole("status");
    expect(live).toHaveTextContent("Lit up, a little");
    screen.getByRole("radio", { name: "Lit up" }).focus();
    await userEvent.keyboard("{ArrowRight}{ArrowUp}");
    expect(live).toHaveTextContent("Lit up, a little");
    await userEvent.keyboard("{ArrowRight}{ArrowRight}{ArrowRight}");
    expect(live).toHaveTextContent("Lit up, quite");
    await userEvent.keyboard(
      "{ArrowUp}{ArrowUp}{ArrowUp}{ArrowUp}{ArrowUp}{ArrowRight}{ArrowRight}",
    );
    expect(live).toHaveTextContent("Lit up, strongly");
    await userEvent.keyboard("{ArrowLeft}".repeat(20));
    expect(screen.getByRole("radio", { name: "Wound up" })).toBeChecked();
    expect(live).toHaveTextContent("Wound up, strongly");
  });

  it("arrows start from the middle when nothing is chosen", async () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    screen.getAllByRole("radio")[0]?.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(spy).toHaveBeenLastCalledWith({ pleasant: 0.1, energy: 0 });
  });

  it("the four parts are a radio group that works without the field", async () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    expect(screen.getAllByRole("radio")).toHaveLength(4);
    await userEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(spy).toHaveBeenLastCalledWith({ pleasant: -0.5, energy: -0.5 });
    expect(screen.getByRole("radio", { name: "Low" })).toBeChecked();
    screen.getByRole("radio", { name: "Wound up" }).focus();
    await userEvent.keyboard(" ");
    expect(spy).toHaveBeenLastCalledWith({ pleasant: -0.5, energy: 0.5 });
    expect(screen.getByRole("radio", { name: "Wound up" })).toBeChecked();
  });

  it("explains the arrow keys as real text", () => {
    render(<Host />);
    expect(screen.getByText("Use the arrow keys to move the point.")).toBeInTheDocument();
  });
});

describe("FeelingField clicks", () => {
  it("a real press on a part keeps the spot it landed on", () => {
    const spy = vi.fn();
    render(<Host spy={spy} />);
    const field = sized();
    const low = screen.getByRole("radio", { name: "Low" });
    fireEvent.pointerDown(low, { clientX: 30, clientY: 170, pointerId: 1 });
    fireEvent.pointerUp(low, { pointerId: 1 });
    fireEvent.click(low, { detail: 1 });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenLastCalledWith({ pleasant: -0.7, energy: -0.7 });
    expect(field).toBeInTheDocument();
  });
});
