import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { fromBase64Url } from "../bytes";
import { ConnectionError, PhoneRequestError } from "../engine";
import { addressReady } from "./Entry";
import { createNativeEngine, SLICE, unframe } from "./engine";
import { applyInsets } from "./insets";
import { fromInfo } from "./store";

afterEach(() => clearMocks());

const kept = { item: { id: "item-1" } };

describe("the native engine", () => {
  it("sends each call to the Rust side and reads its answer", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      seen.push([cmd, args]);
      if (cmd === "call") return { type: "status", status: { readiness: "ready" } };
    });
    const engine = createNativeEngine();
    const status = await engine.status();
    expect(status).toEqual({ readiness: "ready" });
    expect(seen).toContainEqual(["call", { call: { cmd: "getStatus" } }]);
  });

  it("turns the Rust side's failures into the errors the screens know", async () => {
    mockIPC((cmd) => {
      if (cmd === "call") throw { type: "phone", code: "tooMany" };
    });
    await expect(createNativeEngine().getPage()).rejects.toEqual(new PhoneRequestError("tooMany"));
    clearMocks();
    mockIPC((cmd) => {
      if (cmd === "call") throw { type: "connection", state: "removed" };
    });
    const err = await createNativeEngine()
      .listPages()
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ConnectionError);
    expect((err as ConnectionError).state).toBe("removed");
  });

  it("refuses an answer of the wrong kind", async () => {
    mockIPC(() => ({ type: "done" }));
    await expect(createNativeEngine().getItem("item-1")).rejects.toBeInstanceOf(PhoneRequestError);
  });

  it("sends a photo's bytes in slices, then the keep", async () => {
    const seen: Array<[string, unknown]> = [];
    let rebuilt: number[] = [];
    mockIPC((cmd, args) => {
      if (!cmd.startsWith("keep_")) return undefined;
      seen.push([cmd, args]);
      if (cmd === "keep_begin") return 7;
      if (cmd === "keep_part") {
        const slice = (args as { slice: string }).slice;
        rebuilt = rebuilt.concat(Array.from(fromBase64Url(slice)));
      }
      if (cmd === "keep_end") return { type: "kept", kept };
      return null;
    });
    const jpeg = new Uint8Array(SLICE * 2 + 10);
    jpeg.set([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46, 0, 1]);
    const got = await createNativeEngine().keepBytes(
      new Blob([jpeg], { type: "image/jpeg" }),
      "c-1",
      "Blue door",
    );
    expect(got).toEqual(kept);
    expect(seen.map(([c]) => c)).toEqual([
      "keep_begin",
      "keep_part",
      "keep_part",
      "keep_part",
      "keep_end",
    ]);
    expect(seen[0]?.[1]).toMatchObject({
      call: {
        cmd: "keepBytes",
        mime: "image/jpeg",
        size: jpeg.length,
        clientId: "c-1",
        caption: "Blue door",
      },
    });
    expect(seen.slice(1, 4).every(([, a]) => (a as { handle: number }).handle === 7)).toBe(true);
    expect(rebuilt.length).toBe(jpeg.length);
    expect(rebuilt.slice(0, 4)).toEqual([0xff, 0xd8, 0xff, 0xe0]);
  });

  it("cancels a keep whose slice was refused", async () => {
    const cmds: string[] = [];
    mockIPC((cmd) => {
      if (!cmd.startsWith("keep_")) return undefined;
      cmds.push(cmd);
      if (cmd === "keep_begin") return 1;
      if (cmd === "keep_part") throw { type: "phone", code: "invalid" };
      return null;
    });
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46, 0, 1]);
    await expect(
      createNativeEngine().keepBytes(new Blob([jpeg], { type: "image/jpeg" }), "c-3"),
    ).rejects.toEqual(new PhoneRequestError("invalid"));
    await new Promise((r) => setTimeout(r, 0));
    expect(cmds).toEqual(["keep_begin", "keep_part", "keep_cancel"]);
  });

  it("refuses bytes that are not an allowed type before sending", async () => {
    let sent = false;
    mockIPC((cmd) => {
      if (cmd.startsWith("keep_")) sent = true;
    });
    await expect(
      createNativeEngine().keepBytes(new Blob([new Uint8Array([1, 2, 3])]), "c-2"),
    ).rejects.toBeInstanceOf(PhoneRequestError);
    expect(sent).toBe(false);
  });

  it("hears the connection change", async () => {
    mockIPC(() => undefined, { shouldMockEvents: true });
    const engine = createNativeEngine();
    const states: string[] = [];
    engine.subscribe((c) => states.push(c.kind));
    await new Promise((r) => setTimeout(r, 0));
    await emit("connection", { kind: "asleep" });
    expect(states).toEqual(["asleep"]);
    expect(engine.connection().kind).toBe("asleep");
  });

  it("reads a framed original back as a typed blob", async () => {
    const mime = new TextEncoder().encode("image/png");
    const framed = new Uint8Array([mime.length, ...mime, 0x89, 0x50]);
    const blob = unframe(framed.buffer);
    expect(blob.type).toBe("image/png");
    expect(blob.size).toBe(2);
    expect(unframe(Array.from(framed)).size).toBe(2);
    const bad = new TextEncoder().encode("text/html");
    expect(() => unframe(new Uint8Array([bad.length, ...bad]).buffer)).toThrow();
  });
});

describe("the native pieces", () => {
  it("lights Next for four words or a name only", () => {
    expect(addressReady("amber otter lamp quiet")).toBe(true);
    expect(addressReady("Amber-Otter-Lamp-Quiet")).toBe(true);
    expect(addressReady("desk.tail1234.ts.net")).toBe(true);
    expect(addressReady("amber otter lamp")).toBe(false);
    expect(addressReady("desk.")).toBe(false);
    expect(addressReady("127.0.0.1")).toBe(false);
    expect(addressReady("")).toBe(false);
  });

  it("puts the insets on the page as variables", () => {
    const root = document.createElement("div");
    applyInsets({ top: 36.5, right: 0, bottom: -3, left: 0 }, root);
    expect(root.style.getPropertyValue("--ph-inset-top")).toBe("36.5px");
    expect(root.style.getPropertyValue("--ph-inset-bottom")).toBe("0px");
  });

  it("reads the device the Rust side keeps, without keys", () => {
    const d = fromInfo({
      deviceId: "AAAAAAAAAAAAAAAAAAAAAA",
      address: "amber-otter-lamp-quiet.trycloudflare.com",
      homeOrigin: "https://amber-otter-lamp-quiet.trycloudflare.com",
      deviceName: "Pixel",
      computerName: "Desk",
      connection: "simple",
      version: "0.7.2",
      pairedAt: 1,
      lastConnectedAt: null,
    });
    expect(d.deviceId.length).toBe(16);
    expect(d.privateKey).toBeUndefined();
    expect(d.computerName).toBe("Desk");
  });
});
