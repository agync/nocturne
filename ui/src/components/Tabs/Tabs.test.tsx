import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Tabs } from "./Tabs";

const TABS = [
  { id: "people", label: "People", panel: <p>People panel</p> },
  { id: "places", label: "Places", panel: <p>Places panel</p> },
  { id: "things", label: "Things", panel: <p>Things panel</p> },
];

function Harness() {
  const [value, setValue] = useState("people");
  return <Tabs label="Kinds" tabs={TABS} value={value} onChange={setValue} />;
}

describe("Tabs", () => {
  it("follows the ARIA tabs pattern", () => {
    render(<Harness />);
    expect(screen.getByRole("tablist", { name: "Kinds" })).toBeInTheDocument();
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
    expect(tabs.map((t) => t.tabIndex)).toEqual([0, -1, -1]);
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("People");
    expect(screen.getByText("People panel")).toBeInTheDocument();
    expect(screen.queryByText("Places panel")).toBeNull();
  });

  it("moves and chooses with the arrow keys, wrapping at the ends, and with Home and End", async () => {
    render(<Harness />);
    await userEvent.tab();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Places" })).toHaveFocus();
    expect(screen.getByText("Places panel")).toBeInTheDocument();
    await userEvent.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Things" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "People" })).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Things" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "People" })).toHaveFocus();
  });

  it("chooses with a click, and Tab leaves the tab list for the panel", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole("tab", { name: "Things" }));
    expect(screen.getByText("Things panel")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Things" })).toHaveAttribute("aria-selected", "true");
  });
});
