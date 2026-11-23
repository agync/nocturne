import { describe, expect, it, vi } from "vitest";
import type { RemoteView } from "../bindings";
import { createMockEngine } from "./engine";

const manual = (scenario?: string) => createMockEngine({ remote: { auto: false, scenario } });

const state = async (engine: ReturnType<typeof createMockEngine>): Promise<RemoteView["state"]> =>
  (await engine.getRemote()).state;

describe("mock.remote", () => {
  it("full_setup_path: downloads, starts, is on, and adds a phone by scan", async () => {
    const engine = manual();
    const seen: string[] = [];
    engine.on("remote-changed", (view) => seen.push(view.state.type));
    const pairing = vi.fn();
    engine.on("pairing-request", pairing);

    await engine.startRemote("simple", true);
    expect(await state(engine)).toMatchObject({ type: "downloading", receivedBytes: 0 });
    for (let i = 0; i < 4; i += 1) engine.remote.advance();
    expect(await state(engine)).toEqual({ type: "starting" });
    expect((await engine.getRemote()).installed).toEqual(["simple"]);
    engine.remote.advance();
    expect(await state(engine)).toMatchObject({ type: "on" });

    const offer = await engine.beginPairing();
    expect(offer.typedCode.replaceAll(" ", "")).toHaveLength(24);
    expect(offer.qr.modules).toHaveLength(offer.qr.size * offer.qr.size);
    engine.remote.phoneArrives();
    expect(pairing).toHaveBeenCalledOnce();
    const waiting = (await engine.getRemote()).pairing;
    expect(waiting?.phase.type).toBe("waitingForYou");

    await engine.decidePairing(offer.pairingId, true);
    const view = await engine.getRemote();
    expect(view.pairing?.phase).toMatchObject({ type: "allowed", deviceName: "Pixel" });
    expect(view.devices.map((d) => d.name)).toEqual(["Pixel"]);
    expect(seen).toContain("downloading");
    expect(await engine.listRemoteEvents(5)).toEqual(
      expect.arrayContaining([expect.objectContaining({ action: "allowed", deviceName: "Pixel" })]),
    );
  });

  it("relink_after_restart: a Simple restart needs every phone to take the new address", async () => {
    const engine = manual("on");
    const before = await engine.getRemote();
    expect(before.devices).toHaveLength(2);
    engine.remote.restartSimple();
    let view = await engine.getRemote();
    expect(view.state.type).toBe("needsRelink");
    expect(view.devices.every((d) => d.needsAddress)).toBe(true);
    engine.remote.phoneRelinks("pixel");
    view = await engine.getRemote();
    expect(view.state.type).toBe("needsRelink");
    engine.remote.phoneRelinks("work");
    view = await engine.getRemote();
    expect(view.state.type).toBe("on");
    expect(view.devices.every((d) => !d.needsAddress)).toBe(true);
  });

  it("pairing_times_out: an undecided request ends by itself and leaves no phone", async () => {
    const engine = manual("onAlone");
    const offer = await engine.beginPairing();
    engine.remote.phoneArrives();
    engine.remote.timeOut();
    const view = await engine.getRemote();
    expect(view.pairing?.phase.type).toBe("timedOut");
    expect(view.devices).toEqual([]);
    await expect(engine.decidePairing(offer.pairingId, true)).rejects.toMatchObject({
      code: "unavailable",
    });
  });

  it("switch_unpairs: changing the connection removes every phone and turns it off", async () => {
    const engine = manual("on");
    const view = await engine.updateRemoteSettings({
      connection: "mostPrivate",
      autoOff: null,
      keepAwake: null,
    });
    expect(view.devices).toEqual([]);
    expect(view.state).toEqual({ type: "off" });
    expect(view.connection).toBe("mostPrivate");
    expect(await engine.listRemoteEvents(1)).toEqual([
      expect.objectContaining({ action: "switchedConnection" }),
    ]);
  });

  it("walks Most private through sign-in and allowing secure links", async () => {
    const engine = manual();
    await engine.startRemote("mostPrivate", true);
    for (let i = 0; i < 5; i += 1) engine.remote.advance();
    expect(await state(engine)).toMatchObject({ type: "needsSignIn", step: "signIn" });
    engine.remote.advance();
    expect(await state(engine)).toMatchObject({ type: "needsSignIn", step: "allowLinks" });
    engine.remote.advance();
    expect(await state(engine)).toMatchObject({ type: "on" });
  });

  it("pauses and resumes a download", async () => {
    const engine = manual();
    await engine.startRemote("simple", true);
    await engine.pauseRemoteDownload();
    expect(await state(engine)).toMatchObject({ type: "downloading", paused: true });
    engine.remote.advance();
    expect(await state(engine)).toMatchObject({ receivedBytes: 0 });
    await engine.startRemote("simple", true);
    expect(await state(engine)).toMatchObject({ paused: false });
  });
});
