import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { fit } from "./fit";
import { Sheet } from "./Print";

describe("the printed page with a phone's calendar and last night", () => {
  it("prints what is on today and last night like the rest of the page", async () => {
    const page = await withPhones(createMockEngine(), "2026-11-13T07:02:00Z").getPage();
    render(
      <Sheet
        page={page}
        size="a4"
        fitted={fit(page, "a4")}
        printedAt="07:30"
        sentNothing={false}
      />,
    );
    expect(screen.getByRole("heading", { name: "In your calendar today" })).toBeInTheDocument();
    expect(screen.getByText(/Dentist, check-up · Bridge Street Surgery/)).toBeInTheDocument();
    expect(
      screen.getByText(/Last night: 6 h 40 min asleep, 11:50 pm to 7:05 am/),
    ).toBeInTheDocument();
  });
});
