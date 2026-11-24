// Reading on battery with Read now (docs/design/mind-online-setup.md), on This phone.

import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { NativeRoot } from "../native/NativeRoot";
import { nativeMockServices } from "../testing/nativeMock";
import { resetScriptedMind, scriptedMind } from "./testing/homeMock";

afterEach(async () => {
  await new Promise((settle) => setTimeout(settle, 50));
  cleanup();
  clearMocks();
  resetScriptedMind();
  history.replaceState(null, "", "/");
});

async function app(query: string) {
  history.replaceState(null, "", `/?${query}`);
  // The reading and the phone's power come from the address, as the shots set them.
  resetScriptedMind();
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

describe("Reading on battery", () => {
  async function thisPhone(query: string) {
    await app(`m=kept&mind=on&${query}`);
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    return screen.findByRole("region", { name: "Nocturne’s mind online" });
  }

  it("says it waits to charge, and reads now only when the person asks", async () => {
    const group = await thisPhone("power=battery");
    expect(
      await within(group).findByText("answers.example, online · Waiting to charge · 34 to read"),
    ).toBeInTheDocument();
    expect(
      within(group).getByText(
        "Nocturne reads what you keep while this phone charges, so it doesn’t run the battery down.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(group).getByRole("button", { name: "Read now" }));
    const stop = await within(group).findByRole("button", { name: "Stop" });
    expect(
      within(group).getByText("answers.example, online · Reading now · 12 of 46"),
    ).toBeVisible();
    expect(within(group).getByRole("progressbar")).toHaveAttribute(
      "aria-valuetext",
      "12 of 46 read",
    );
    await waitFor(() => expect(stop).toHaveFocus());
    fireEvent.click(stop);
    const again = await within(group).findByRole("button", { name: "Read now" });
    await waitFor(() => expect(again).toHaveFocus());
    expect(scriptedMind.reading.stopped?.why).toBe("person");
  });

  it("offers Read anyway once Read now stopped at 20 per cent", async () => {
    const group = await thisPhone("power=low&done=31&count=89");
    expect(
      await within(group).findByText(
        "Read now stopped at 20% battery, after 31. The rest is read while this phone charges.",
      ),
    ).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Read now" })).toBeNull();
    fireEvent.click(within(group).getByRole("button", { name: "Read anyway" }));
    expect(await within(group).findByText("Reading anyway, down to 5% battery.")).toBeVisible();
  });

  it("offers nothing at 5 per cent", async () => {
    const group = await thisPhone("power=floor&done=40&count=89");
    expect(
      await within(group).findByText("Read now works above 5% battery, or while charging."),
    ).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: /Read/ })).toBeNull();
  });

  it("keeps today's words on the charger", async () => {
    const group = await thisPhone("");
    expect(
      await within(group).findByText("answers.example, online · Reading · 12 of 46"),
    ).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Read now" })).toBeNull();
  });

  it("says the limit holds, with Change limit", async () => {
    const group = await thisPhone("power=battery&trouble=paused");
    expect(
      await within(group).findByText("answers.example, online · Paused by your limit · 34 to read"),
    ).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "Change limit" })).toBeVisible();
    expect(within(group).queryByRole("button", { name: "Read now" })).toBeNull();
  });

  it("can't read now while this phone is offline, and says why", async () => {
    const group = await thisPhone("power=battery&trouble=offline");
    const readNow = await within(group).findByRole("button", { name: "Read now" });
    expect(readNow).toHaveAttribute("aria-disabled", "true");
    expect(readNow).toHaveAccessibleDescription("Read now works once this phone is online.");
  });
});
