import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { nativeMockServices } from "../testing/nativeMock";
import type { ThisPhoneSources } from "./api";
import { applyInsets } from "./insets";
import { NativeApp, updateFor } from "./NativeApp";
import { NativeRoot } from "./NativeRoot";
import { calendarValue, healthValue, Offer, offered } from "./screens/Offer";
import { UpdateCard } from "./screens/UpdateCard";
import { nativeChannel } from "./sources";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
  history.replaceState(null, "", "/");
});

async function app(query = "") {
  history.replaceState(null, "", `/?${query}`);
  const services = await nativeMockServices();
  render(<NativeRoot services={services} />);
  return services;
}

const nav = () => screen.getByRole("navigation", { name: "Nocturne" });

describe("the app's bar, Keep and This phone", () => {
  it("opens on Today with the bar at the bottom and Keep floating", async () => {
    await app();
    expect(await screen.findByText("Connected to Home computer")).toBeInTheDocument();
    const today = within(nav()).getByRole("button", { name: "Today" });
    expect(today).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { level: 1, name: "Today" })).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: "In your calendar today · 3" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Last night" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByLabelText(/Type something to keep/)).toBeInTheDocument();
  });

  it("moves between the tabs, and Ask has no Keep button", async () => {
    await app();
    await screen.findByText("Connected to Home computer");
    fireEvent.click(within(nav()).getByRole("button", { name: "Journal" }));
    expect(await screen.findByText("Woke · 6 h 40 min asleep")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "Journal" })).toBeInTheDocument();
    fireEvent.click(within(nav()).getByRole("button", { name: "Ask" }));
    expect(within(nav()).getByRole("button", { name: "Ask" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.queryByRole("button", { name: "Keep" })).toBeNull();
  });

  it("opens This phone from the top bar", async () => {
    await app();
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    const brings = await screen.findByRole("region", { name: "What this phone brings" });
    expect(await within(brings).findByText("On · 3 calendars")).toBeInTheDocument();
    expect(within(brings).getByText("On · 6 kinds")).toBeInTheDocument();
    expect(brings.textContent).toContain("4 calendar changes, 18 readings");
    expect(screen.getByText("Your computer: Nocturne 0.1.0")).toBeInTheDocument();
    expect(screen.getByText("And when you open it.")).toBeInTheDocument();
  });

  it("disconnects from This phone and goes back to Welcome", async () => {
    await app();
    fireEvent.click(await screen.findByRole("button", { name: "This phone" }));
    fireEvent.click(await screen.findByRole("button", { name: "Disconnect this phone…" }));
    fireEvent.click(await screen.findByRole("button", { name: "Disconnect" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne remembers your day" }),
    ).toBeInTheDocument();
  });

  it("says the computer is away on Today, and Keep still works", async () => {
    await app("s=asleep");
    expect(
      await screen.findByText(
        "Today’s page is back when your computer is. This phone keeps no copy.",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText(/^2 waiting · Last connected/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep" })).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Has your computer got a new address?" }),
    ).toBeInTheDocument();
  });

  it("asks this phone to update when the computer wants a newer app", async () => {
    const services = await app("s=outdated");
    expect(
      await screen.findByRole("heading", { name: "Update Nocturne on this phone" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update in Google Play" })).toBeInTheDocument();
    expect(
      screen.getByText(`Your computer: Nocturne 0.9.0 · This phone: Nocturne ${services.version}`),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep" })).toBeInTheDocument();
  });

  it("asks for the computer to be updated when this app is newer", async () => {
    const services = await app("s=computerOutdated");
    expect(
      await screen.findByRole("heading", { name: "Update Nocturne on your computer" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "This phone has a newer Nocturne than your computer. What you keep waits here until your computer is updated.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update in Google Play" })).toBeNull();
    expect(
      screen.getByText(`Your computer: Nocturne 0.1.0 · This phone: Nocturne ${services.version}`),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^Connecting to/)).toBeNull();
  });

  it("shows the removed screen when the computer removed this phone", async () => {
    await app("s=removed");
    expect(
      await screen.findByRole("heading", { name: "This phone was removed from Nocturne." }),
    ).toBeInTheDocument();
  });

  it("opens the share sheet for what another app shared", async () => {
    await app("n=share");
    const sheet = await screen.findByRole("dialog", { name: "Keep in Nocturne" });
    expect(within(sheet).getByText("https://example.org/school-trip-letter")).toBeInTheDocument();
  });
});

const sources = (over: Partial<ThisPhoneSources> = {}): ThisPhoneSources => ({
  calendar: "on",
  calendars: 2,
  health: "notBrought",
  kinds: 0,
  lastSent: null,
  calendarLastSent: null,
  healthLastSent: null,
  healthFrom: null,
  computerTakes: ["calendar", "health"],
  calendarTooMany: false,
  ...over,
});

describe("Bring more from this phone", () => {
  it("says what each source stands at", () => {
    expect(calendarValue(sources())).toBe("On · 2 calendars");
    expect(calendarValue(sources({ calendars: 1 }))).toBe("On · 1 calendar");
    expect(calendarValue(sources({ calendar: "notBrought" }))).toBe("Not brought");
    expect(calendarValue(sources({ calendar: "notBrought" }), true)).toBe("Choose");
    expect(calendarValue(sources({ calendar: "offInSettings" }))).toBe(
      "Off in your phone’s settings",
    );
    expect(healthValue(sources({ health: "needsNewerComputer" }))).toBe(
      "Needs a newer Nocturne on your computer.",
    );
    expect(healthValue(sources({ health: "on", kinds: 6 }))).toBe("On · 6 kinds");
  });

  it("hides what the computer does not take", () => {
    expect(offered(sources({ computerTakes: ["calendar"] }), "health")).toBe(false);
    expect(offered(sources({ calendar: "needsNewerComputer" }), "calendar")).toBe(false);
  });

  it("skips the offer when the computer takes neither", async () => {
    const onDone = vi.fn();
    const channel = {
      sources: async () => sources({ computerTakes: [] }),
    } as unknown as Parameters<typeof Offer>[0]["channel"];
    render(
      <Offer
        channel={channel}
        connected
        chosen={new Set()}
        onCalendar={vi.fn()}
        onHealth={vi.fn()}
        onDone={onDone}
      />,
    );
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("opens on the offer after pairing, whatever an earlier screen left in history", async () => {
    history.replaceState({ view: { name: "tab", tab: "journal" } }, "", "/?n=fresh");
    const services = await nativeMockServices();
    const device = await services.store.device();
    if (!device) throw new Error("paired");
    render(
      <NativeApp
        services={services}
        channel={nativeChannel}
        initial={device}
        offer
        onUnpaired={vi.fn()}
      />,
    );
    expect(
      await screen.findByRole("heading", { name: "Bring more from this phone?" }),
    ).toBeInTheDocument();
  });

  it("opens on Today when the phone alone left its own screen in history before a move", async () => {
    history.replaceState({ view: { name: "mind" } }, "", "/");
    history.pushState({ view: { name: "moveFound" } }, "");
    const services = await nativeMockServices();
    const device = await services.store.device();
    if (!device) throw new Error("paired");
    render(
      <NativeApp
        services={services}
        channel={nativeChannel}
        initial={device}
        offer={false}
        onUnpaired={vi.fn()}
      />,
    );
    expect(await screen.findByRole("heading", { level: 1, name: "Today" })).toBeInTheDocument();
    window.dispatchEvent(new PopStateEvent("popstate", { state: { view: { name: "mind" } } }));
    expect(await screen.findByRole("heading", { level: 1, name: "Today" })).toBeInTheDocument();
  });

  it("comes back to the offer with the calendar refused", async () => {
    await app("n=calendarRefused&v=offer");
    fireEvent.click(await screen.findByRole("button", { name: /Your calendar/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(
      await screen.findByRole("heading", { name: "Your calendar stays on this phone." }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(await screen.findByText("Not brought")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(await screen.findByRole("navigation", { name: "Nocturne" })).toBeInTheDocument();
  });
});

describe("insets", () => {
  it("marks the page while the keyboard is up", () => {
    const root = document.createElement("div");
    applyInsets({ top: 24, right: 0, bottom: 16, left: 0, keyboard: 300 }, root);
    expect(root.style.getPropertyValue("--ph-inset-keyboard")).toBe("300px");
    expect(root.classList.contains("ph-keyboard")).toBe(true);
    applyInsets({ top: 24, right: 0, bottom: 16, left: 0 }, root);
    expect(root.classList.contains("ph-keyboard")).toBe(false);
  });
});

describe("which side must update", () => {
  const device = {
    address: "amber-otter-lamp-quiet.trycloudflare.com",
    deviceName: "Pixel",
    version: "0.8.0",
  } as Parameters<typeof updateFor>[1];

  it("is nobody while connected or away", () => {
    expect(updateFor({ kind: "asleep" }, device, "0.9.0")).toBeNull();
    expect(updateFor({ kind: "connecting" }, device, "0.9.0")).toBeNull();
  });

  it("is this phone, with the computer's version from its hello or as last seen", () => {
    expect(
      updateFor({ kind: "outdated", minBuild: 3, computer: "1.2.0" }, device, "0.9.0"),
    ).toEqual({ kind: "phone", computer: "1.2.0", phone: "0.9.0", ios: false });
    expect(updateFor({ kind: "outdated", minBuild: 3 }, device, "0.9.0")).toEqual({
      kind: "phone",
      computer: "0.8.0",
      phone: "0.9.0",
      ios: false,
    });
  });

  it("is the computer when this app is newer", () => {
    expect(updateFor({ kind: "computerOutdated", computer: "0.7.2" }, device, "0.9.0")).toEqual({
      kind: "computer",
      computer: "0.7.2",
      phone: "0.9.0",
      ios: false,
    });
  });

  it("sends an iPhone to the App Store, and is said politely as it appears", () => {
    const update = updateFor({ kind: "outdated", minBuild: 3 }, device, "0.9.0", true);
    expect(update?.ios).toBe(true);
    render(<UpdateCard update={update as NonNullable<typeof update>} />);
    expect(screen.getByRole("button", { name: "Update in the App Store" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update in Google Play" })).toBeNull();
    expect(
      screen.getByRole("heading", { name: "Update Nocturne on this phone" }).closest("section"),
    ).toHaveAttribute("aria-live", "polite");
  });
});
