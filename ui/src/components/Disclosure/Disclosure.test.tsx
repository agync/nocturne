import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Disclosure } from "./Disclosure";

describe("Disclosure", () => {
  it("keeps its words out of the page until it opens", async () => {
    render(
      <Disclosure label="Everything that is sent">
        <p>The full detail.</p>
      </Disclosure>,
    );
    const toggle = screen.getByRole("button", { name: "Everything that is sent" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).not.toHaveAttribute("aria-controls");
    expect(screen.queryByText("The full detail.")).toBeNull();
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const panel = document.getElementById(toggle.getAttribute("aria-controls") ?? "");
    expect(panel).toHaveTextContent("The full detail.");
    await userEvent.click(toggle);
    expect(toggle).not.toHaveAttribute("aria-controls");
    expect(screen.queryByText("The full detail.")).toBeNull();
  });

  it("points its chevron first and leaves the name as the words alone", () => {
    render(<Disclosure label="Why this one?">x</Disclosure>);
    const toggle = screen.getByRole("button", { name: "Why this one?" });
    expect(toggle.firstElementChild).toHaveAttribute("data-chevron");
  });
});
