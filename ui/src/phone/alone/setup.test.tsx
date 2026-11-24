// Step 2 of setting up the mind online (docs/design/mind-online-setup.md): the service's list.

import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { resetScriptedMind } from "./testing/homeMock";

afterEach(async () => {
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  resetScriptedMind();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });

async function toDetails() {
  fireEvent.click(await screen.findByRole("button", { name: "Choose a mind" }));
  await screen.findByRole("heading", { name: "Where should Nocturne’s mind be?" });
  fireEvent.click(button("Continue"));
  await screen.findByRole("heading", {
    name: "Have Nocturne’s mind work online, with a service you choose.",
  });
  fireEvent.click(button("Continue"));
  await screen.findByRole("heading", { name: "Your service’s details" });
}

function fill(url: string, key: string) {
  fireEvent.change(screen.getByLabelText("API URL"), { target: { value: url } });
  fireEvent.change(screen.getByLabelText("API key"), { target: { value: key } });
}

describe("Step 2, the service's list", () => {
  it("waits for the address and key, then looks only when asked", async () => {
    await app("m=fresh");
    await toDetails();
    const name = screen.getByLabelText("Model name");
    expect(name).toHaveAttribute("placeholder", "Type it, or choose from the list");
    const pick = button("Choose from the service’s list…");
    expect(pick).toHaveAttribute("aria-disabled", "true");
    expect(pick).toHaveAccessibleDescription("Fill in the API URL and key first.");
    fireEvent.click(pick);
    expect(screen.queryByRole("dialog")).toBeNull();

    fill("https://answers.example/v1", "sk-1");
    fireEvent.click(button("Choose from answers.example’s list…"));
    const sheet = await screen.findByRole("dialog", { name: "answers.example’s list" });
    expect(within(sheet).getByText("6 names, A to Z")).toBeInTheDocument();
    // A short list has no search, and runs A to Z.
    expect(within(sheet).queryByRole("searchbox")).toBeNull();
    const names = within(sheet)
      .getAllByRole("radio")
      .map((r) => r.closest("label")?.textContent);
    expect(names).toEqual([
      "reader-large",
      "reader-large-2026-09",
      "reader-small",
      "sight-reader-1",
      "writer-mini",
      "writer-pro",
    ]);
    expect(
      within(sheet).getByText(
        "Not here? Close the list and type the name from the service’s website.",
      ),
    ).toBeInTheDocument();

    fireEvent.click(within(sheet).getByRole("radio", { name: "reader-small" }), { detail: 1 });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(name).toHaveValue("reader-small");
    expect(name).toHaveAccessibleDescription("On answers.example’s list.");
    // Choosing is typing: the check still decides.
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(button("Check"));
    expect(await screen.findByText("answers.example answered. It works.")).toBeInTheDocument();
  });

  it("warns about a name not on the list, and offers the one that differs in capitals", async () => {
    await app("m=fresh");
    await toDetails();
    fill("https://answers.example/v1", "sk-1");
    fireEvent.click(button("Choose from answers.example’s list…"));
    const sheet = await screen.findByRole("dialog", { name: "answers.example’s list" });
    fireEvent.click(within(sheet).getByRole("button", { name: "Close" }));
    const name = screen.getByLabelText("Model name");
    fireEvent.change(name, { target: { value: "reader-smal" } });
    expect(name).toHaveAccessibleDescription(
      "“reader-smal” isn’t on answers.example’s list. Check the spelling, or choose from the list. Capitals and dashes matter.",
    );
    fireEvent.change(name, { target: { value: "Reader-Small" } });
    expect(
      screen.getByText("“Reader-Small” isn’t on answers.example’s list, but “reader-small” is."),
    ).toBeInTheDocument();
    fireEvent.click(button("Use “reader-small”"));
    expect(name).toHaveValue("reader-small");
    // A name not on the list still checks: the service's answer decides.
    fireEvent.change(name, { target: { value: "reader-old" } });
    fireEvent.click(button("Check"));
    expect(await screen.findByText("answers.example answered. It works.")).toBeInTheDocument();
  });

  it("tells a refused key at the list, pointing at the key", async () => {
    await app("m=fresh");
    await toDetails();
    fill("https://answers.example/v1", "refused");
    fireEvent.click(button("Choose from answers.example’s list…"));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("answers.example refused the key.");
    expect(alert).toHaveTextContent(
      "Check you copied all of it, then try the list again. Some services also need credit on the account before a key works.",
    );
    expect(button("Try the list again")).toBeVisible();
    // A new key drops what the look found.
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-2" } });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says when the address gives no list, and leaves the name to type", async () => {
    await app("m=fresh");
    await toDetails();
    fill("https://nolist.example/v1", "sk-1");
    fireEvent.click(button("Choose from nolist.example’s list…"));
    expect(
      await screen.findByText("nolist.example didn’t give a list at that address."),
    ).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Model name")).toHaveFocus());
  });

  it("searches a long list, marks what matches, and uses a name as typed", async () => {
    await app("m=fresh&names=long");
    await toDetails();
    fill("https://answers.example/v1", "sk-1");
    fireEvent.click(button("Choose from answers.example’s list…"));
    const sheet = await screen.findByRole("dialog", { name: "answers.example’s list" });
    const search = within(sheet).getByRole("searchbox", { name: "Search the list" });
    await waitFor(() => expect(search).toHaveFocus());
    fireEvent.change(search, { target: { value: "small" } });
    expect(within(sheet).getAllByRole("radio")).toHaveLength(7);
    expect(sheet.querySelectorAll("mark")).toHaveLength(7);
    // The marked part does not split the name a screen reader reads.
    expect(within(sheet).getByRole("radio", { name: "reader-small" })).toHaveAttribute(
      "aria-label",
      "reader-small",
    );
    expect(
      await within(sheet).findByText("7 of 214 have “small” in them.", {}, { timeout: 2000 }),
    ).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "reader-tiny" } });
    expect(
      within(sheet).getByText("No name on the list has “reader-tiny” in it."),
    ).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Use “reader-tiny” as typed" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByLabelText("Model name")).toHaveValue("reader-tiny");
  });
});
