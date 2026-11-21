import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PairingLike, Services } from "../services";
import { mockServices } from "../testing/mockServices";
import { Pair } from "./Pair";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  history.replaceState(null, "", "/");
});

describe("pairing in the app", () => {
  const pairing: PairingLike = {
    spentAlready: false,
    connect: async (_name, onWords) => {
      onWords(["harbour", "maple", "lantern"]);
      const services = await mockServices();
      const device = await services.store.device();
      return { kind: "allowed", device: device as NonNullable<typeof device> };
    },
  };

  async function pair(native: boolean, deviceName?: () => Promise<string | null>) {
    history.replaceState(null, "", "/?s=connected");
    const services: Services = {
      ...(await mockServices()),
      platform: { ios: false, standalone: true, native },
      deviceName,
    };
    render(<Pair services={services} pairing={pairing} onPaired={vi.fn()} onExit={vi.fn()} />);
  }

  it("names the phone as the phone names itself", async () => {
    await pair(true, async () => "Sam’s Pixel");
    await waitFor(() => expect(screen.getByLabelText(/name/i)).toHaveValue("Sam’s Pixel"));
    expect(screen.queryByText(/Got the Nocturne app/)).toBeNull();
  });

  it("says Continue once allowed, with no home screen steps", async () => {
    await pair(true);
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(await screen.findByText("Keep, ask and read your page from here.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(screen.queryByText(/Add to Home screen/)).toBeNull();
  });

  it("tells the phone page about the app", async () => {
    await pair(false);
    expect(
      screen.getByText("Got the Nocturne app? Open it and choose Scan the code instead."),
    ).toBeInTheDocument();
  });
});
