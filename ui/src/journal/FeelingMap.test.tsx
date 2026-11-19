import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CheckIn } from "../engine/bindings";
import { FeelingMap } from "./FeelingMap";

const checkin = (id: string, pleasant: number, energy: number): CheckIn => ({
  id,
  keptAt: "2026-11-19T18:10:00Z",
  day: "2026-11-19",
  feeling: { part: "settled", point: { pleasant, energy }, words: ["calm"] },
  line: null,
  urge: null,
  about: null,
  tried: [],
  fromDevice: null,
  mentions: [],
  leftOut: false,
});

describe("FeelingMap", () => {
  it("places a dot where the person put it and opens its check-in", () => {
    const onOpen = vi.fn();
    render(
      <FeelingMap
        checkins={[checkin("c1", 0.5, -0.5)]}
        onOpen={onOpen}
        dotLabel={() => "Settled, calm"}
      />,
    );
    const dot = screen.getByRole("button", { name: "Settled, calm" });
    expect(dot.style.left).toBe("75%");
    expect(dot.style.top).toBe("75%");
    fireEvent.click(dot);
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "c1" }));
  });

  it("keeps dots out of the accessibility tree when they open nothing", () => {
    render(<FeelingMap checkins={[checkin("c1", 0, 0)]} />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
