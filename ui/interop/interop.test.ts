// I1: the Rust test phone and the TypeScript phone client each talk to the other side's real
// implementation, over WebSocket and long-poll, with a scanned and a typed code.
//
// - TypeScript phone against the real computer (the app on the mock runtime, the stand-in helper
//   relaying to its socket): `Driver`.
// - Rust phone against the phone app's own computer (WebCrypto), behind a loopback socket:
//   `serveComputer`.

import { spawn } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { afterAll, beforeAll, describe, expect, test } from "vitest";
import type { AskDelta } from "../src/engine/bindings";
import type { Env, WebSocketLike } from "../src/phone/channel";
import { createPhoneEngine } from "../src/phone/client";
import { offerFromFragment, Pairing } from "../src/phone/pairing";
import { CHUNK } from "../src/phone/protocol";
import { FakeComputer } from "../src/phone/testing/fakeComputer";
import { memoryKeyStore } from "../src/phone/testing/memoryStore";
import { builtTest, Driver, HOST, repo } from "./driver";
import { serveComputer } from "./serveComputer";

/** The phone's network, with the computer's address mapped to the loopback port that reaches it. */
function routed(port: number): Env {
  const from = `https://${HOST}`;
  const to = `http://127.0.0.1:${port}`;
  class Socket extends globalThis.WebSocket {
    constructor(url: string) {
      super(url.replace(`wss://${HOST}`, `ws://127.0.0.1:${port}`));
    }
  }
  return {
    fetch: (input, init) => globalThis.fetch(String(input).replace(from, to), init),
    WebSocket: Socket as unknown as new (url: string) => WebSocketLike,
    now: () => Date.now(),
  };
}

function photo(marker: string, len: number): Uint8Array<ArrayBuffer> {
  const head = [0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46];
  const bytes = new Uint8Array(new ArrayBuffer(head.length + marker.length + len));
  bytes.set(head);
  bytes.set(new TextEncoder().encode(marker), head.length);
  for (let i = 0; i < len; i++) bytes[head.length + marker.length + i] = (i * 31) % 251;
  return bytes;
}

type Item = { id: string; kind: string; text: string | null; originalPath: string | null };

const COMBOS = [
  { how: "scanned", mode: "ws" },
  { how: "scanned", mode: "poll" },
  { how: "typed", mode: "ws" },
  { how: "typed", mode: "poll" },
] as const;

describe("TypeScript phone, Rust computer", () => {
  let d: Driver;
  beforeAll(async () => {
    d = await Driver.start();
  });
  afterAll(async () => {
    await d?.stop();
  });

  for (const { how, mode } of COMBOS) {
    test(`${how} pairing, then a session over ${mode === "ws" ? "WebSocket" : "long-poll"}`, async () => {
      const before = existsSync(d.relay) ? statSync(d.relay).size : 0;
      const env = routed(d.info.publicPort);
      const store = memoryKeyStore();
      const ctx = {
        env,
        host: HOST,
        origin: `https://${HOST}`,
        store,
        preferPoll: mode === "poll",
      };
      const offer = (await d.call("offer")) as {
        pairingId: string;
        link: string;
        typedCode: string;
      };
      const name = `TS ${how} ${mode}`;
      const pairing =
        how === "scanned"
          ? await Pairing.fromLink(
              offerFromFragment(new URL(offer.link).hash) as NonNullable<
                ReturnType<typeof offerFromFragment>
              >,
              ctx,
            )
          : await Pairing.fromCode(offer.typedCode, ctx);
      let phoneWords: string[] = [];
      const result = pairing.connect(name, (w) => {
        phoneWords = w;
      });
      const asked = (await d.call("request")) as {
        pairingId: string;
        deviceName: string;
        words: string[];
      };
      expect(asked.deviceName).toBe(name);
      await expect.poll(() => phoneWords.length, { timeout: 10_000 }).toBe(3);
      expect(phoneWords).toEqual(asked.words);
      await d.call("decide", { pairingId: asked.pairingId, allow: true });
      const outcome = await result;
      expect(outcome.kind).toBe("allowed");

      const engine = createPhoneEngine(store, () => HOST, { env, preferPoll: mode === "poll" });
      try {
        const hello = await engine.hello();
        expect(hello.deviceName).toBe(name);

        const marker = `PLAIN-${how}-${mode}-${Date.now()}`;
        const kept = await engine.keepText(`Parcel for ${marker}`, `c-${how}-${mode}-text`);
        expect(kept.duplicate).toBe(false);
        const again = await engine.keepText(`Parcel for ${marker}`, `c-${how}-${mode}-text`);
        expect(again.duplicate).toBe(true);
        expect(again.item.id).toBe(kept.item.id);

        const bytes = photo(`PHOTO-${how}-${mode}`, 2 * CHUNK + 777);
        const shot = await engine.keepBytes(
          new Blob([bytes], { type: "image/jpeg" }),
          `c-${how}-${mode}-photo`,
          "from the phone",
        );
        expect(shot.item.hasOriginal).toBe(true);

        const items = (await d.call("items")) as Item[];
        expect(items.filter((i) => i.text?.includes(marker))).toHaveLength(1);
        const stored = items.find((i) => i.id === shot.item.id);
        expect(stored?.originalPath).toBeTruthy();
        expect(
          Buffer.from(readFileSync(stored?.originalPath as string)).equals(Buffer.from(bytes)),
        ).toBe(true);

        const deltas: AskDelta[] = [];
        await engine.ask("When is the boiler serviced?", (x) => deltas.push(x));
        expect(deltas.at(-1)?.phase).toBe("done");
        expect(deltas.map((x) => x.textDelta ?? "").join("")).toContain("boiler");
        const cited = deltas.flatMap((x) => x.receipts.map((r) => r.itemId));
        expect(cited.length).toBeGreaterThan(0);
        const original = await engine.original(cited[0] as string);
        const seen = Buffer.from(await original.arrayBuffer());
        expect(seen.includes("DESK-PHOTO-MARKER-5c1e")).toBe(true);

        const page = await engine.getPage();
        expect(page.edition).toBe("phone");
        expect((await engine.listPages()).length).toBeGreaterThan(0);
      } finally {
        engine.close();
      }
      // What the relay carried says which transport was really used, not which was asked for.
      const carried = readFileSync(d.relay).subarray(before).toString("latin1").toLowerCase();
      if (mode === "ws") {
        expect(carried).toContain("upgrade: websocket");
        expect(carried).not.toContain("post /s ");
      } else {
        expect(carried).not.toContain("websocket");
        expect(carried).toContain("post /s ");
        expect(carried).toContain("get /s?sid=");
      }
    });
  }
});

interface PhoneRun {
  ok?: {
    words: string[];
    hello: { computerName: string; deviceName: string };
    status: { type: string };
    kept: { type: string; kept: { duplicate: boolean } };
    photoKept: { type: string };
    photoSize: number;
    streamed: string;
    phases: string[];
    pages: { type: string; pages: unknown[] };
  };
  error?: string;
}

function runRustPhone(scenario: Record<string, unknown>): Promise<PhoneRun> {
  return new Promise((done, fail) => {
    const child = spawn(
      builtTest("away_phone"),
      ["--ignored", "--nocapture", "--exact", "phone", "--test-threads=1"],
      {
        cwd: `${repo}/src-tauri`,
        env: { ...process.env, AWAY_SCENARIO: JSON.stringify(scenario) },
      },
    );
    let out = "";
    let err = "";
    child.stdout.on("data", (c) => {
      out += c;
    });
    child.stderr.on("data", (c) => {
      err += c;
    });
    child.on("exit", () => {
      const at = out.indexOf("AWAYPHONE ");
      if (at < 0) return fail(new Error(`no result: ${out.slice(-400)} ${err.slice(-400)}`));
      done(JSON.parse(out.slice(at + 10).split("\n")[0] ?? "") as PhoneRun);
    });
  });
}

describe("Rust phone, TypeScript computer", () => {
  for (const { how, mode } of COMBOS) {
    test(`${how} pairing, then a session over ${mode === "ws" ? "WebSocket" : "long-poll"}`, async () => {
      const computer = await FakeComputer.create({ host: HOST });
      computer.autoDecide = "allow";
      const served = await serveComputer(computer);
      try {
        const offer = await computer.offer(how);
        const name = `Rust ${how} ${mode}`;
        const run = await runRustPhone({
          port: served.port,
          mode,
          name,
          host: HOST,
          ...(how === "scanned" ? { link: offer.link } : { code: offer.code }),
          text: `Parcel for RUSTPHONE-${how}-${mode}`,
          marker: `RUSTPHOTO-${how}-${mode}`,
        });
        expect(run.error).toBeUndefined();
        const ok = run.ok as NonNullable<PhoneRun["ok"]>;
        expect(ok.words).toEqual(computer.lastWords);
        expect(ok.hello.deviceName).toBe(name);
        expect(ok.status.type).toBe("status");
        expect(ok.kept.type).toBe("kept");
        expect(ok.photoKept.type).toBe("kept");
        expect(ok.streamed).toBe("Swimming is on **Monday**.");
        expect(ok.phases.at(-1)).toBe("Done");
        expect(ok.pages.type).toBe("pages");
        const items = [...computer.items.values()];
        expect(items.filter((i) => i.text?.includes("RUSTPHONE"))).toHaveLength(1);
        const stored = items.find((i) => i.bytes);
        expect(stored?.bytes?.length).toBe(ok.photoSize);
        expect(computer.refusals).toEqual([]);
        expect(served.upgrades > 0).toBe(mode === "ws");
      } finally {
        await served.close();
      }
    });
  }
});
