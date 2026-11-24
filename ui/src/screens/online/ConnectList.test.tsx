import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openOnline } from "./testing";

afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const ADVANCED = "/settings/advanced/online?from=online";
const CONNECT = "Connect to the service you chose.";

const click = (name: string | RegExp) => userEvent.click(screen.getByRole("button", { name }));

async function enter(label: RegExp, text: string) {
  const field = screen.getByLabelText(label);
  await userEvent.clear(field);
  await userEvent.click(field);
  await userEvent.paste(text);
}

describe("Connect, the service's list", () => {
  it("looks only when asked, and choosing puts the name in the field", async () => {
    const { engine } = await openOnline(ADVANCED, "off", CONNECT);
    const pick = screen.getByRole("button", { name: "Choose from the service’s list…" });
    expect(pick).toHaveAttribute("aria-disabled", "true");
    expect(pick).toHaveAccessibleDescription("Fill in the address and key first.");
    expect(screen.getByLabelText(/^Which one to use/)).toHaveAttribute(
      "placeholder",
      "Type it, or choose from the list",
    );
    await enter(/^Address/, "https://api.example.com/v1");
    await enter(/^Key/, "sk-example-not-a-real-key");
    expect(engine.online.lists).toHaveLength(0);
    await click("Choose from api.example.com’s list…");
    const dialog = await screen.findByRole("dialog", { name: "api.example.com’s list" });
    expect(engine.online.lists).toHaveLength(1);
    expect(within(dialog).getByText("6 names, A to Z")).toBeInTheDocument();
    const rows = within(dialog).getAllByRole("radio");
    expect(rows.map((r) => r.textContent)).toEqual([
      "reader-large",
      "reader-large-2026-09",
      "reader-small",
      "sight-reader-1",
      "writer-mini",
      "writer-pro",
    ]);
    // The arrows move without choosing; Enter chooses.
    await waitFor(() => expect(rows[0]).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(rows[2]).toHaveFocus();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const name = screen.getByLabelText(/^Which one to use/);
    expect(name).toHaveValue("reader-small");
    expect(screen.getByText("On api.example.com’s list.")).toBeInTheDocument();
    // Tapping again opens the same list: no second look.
    await click("Choose from api.example.com’s list…");
    await screen.findByRole("dialog", { name: "api.example.com’s list" });
    expect(engine.online.lists).toHaveLength(1);
  });

  it("names a refused key at the list, as the check words it", async () => {
    await openOnline(ADVANCED, "off", CONNECT);
    await enter(/^Address/, "https://api.example.com/v1");
    await enter(/^Key/, "refused");
    await click("Choose from api.example.com’s list…");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(
      "Check you copied all of it, then try the list again. Some services also need credit on the account before a key works.",
    );
    expect(screen.getByRole("button", { name: "Try the list again" })).toBeVisible();
  });
});
