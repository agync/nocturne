import type { Page } from "@playwright/test";
import type { NetEvent } from "./fixtures";
import { expect } from "./fixtures";
import { type Computer, HOST } from "./world";

export type How = "scanned" | "typed";
export type Mode = "ws" | "poll";

export const JPEG_HEAD = [0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46];

/** A JPEG-shaped file that carries a marker the relay must never see. */
export function photo(marker: string, size: number): Buffer {
  const out = Buffer.alloc(size);
  Buffer.from(JPEG_HEAD).copy(out);
  out.write(marker, JPEG_HEAD.length, "latin1");
  for (let i = JPEG_HEAD.length + marker.length; i < size; i++) out[i] = (i * 31) % 251;
  return out;
}

/**
 * Watches what the page's own network does, so a test can say when a held long-poll or a
 * WebSocket ended. Optionally makes WebSockets fail, as a network that blocks them does.
 */
export async function instrument(page: Page, mode: Mode): Promise<void> {
  await page.addInitScript((blockSockets: boolean) => {
    const net: NetEvent[] = [];
    (window as unknown as { __net: NetEvent[] }).__net = net;
    const realFetch = window.fetch.bind(window);
    window.fetch = async (input, init) => {
      const url = String(input instanceof Request ? input.url : input);
      const poll = url.includes("/s?sid=") && (init?.method ?? "GET") === "GET";
      if (poll) net.push({ t: Date.now(), kind: "poll-start" });
      try {
        return await realFetch(input, init);
      } finally {
        if (poll) net.push({ t: Date.now(), kind: "poll-end" });
      }
    };
    const RealSocket = window.WebSocket;
    if (blockSockets) {
      // A network that does not let WebSockets through: the socket never opens and sends nothing.
      window.WebSocket = class {
        binaryType = "blob";
        bufferedAmount = 0;
        onopen: (() => void) | null = null;
        onmessage: (() => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        constructor() {
          setTimeout(() => {
            this.onerror?.();
            this.onclose?.();
          }, 0);
        }
        send() {}
        close() {}
      } as unknown as typeof WebSocket;
      return;
    }
    window.WebSocket = class extends RealSocket {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        this.addEventListener("open", () => net.push({ t: Date.now(), kind: "ws-open" }));
        this.addEventListener("close", () => net.push({ t: Date.now(), kind: "ws-close" }));
      }
    } as typeof WebSocket;
  }, mode === "poll");
}

export async function netEvents(page: Page): Promise<NetEvent[]> {
  return page.evaluate(() => (window as unknown as { __net: NetEvent[] }).__net);
}

/** Pair the phone in `page` with the computer, by a scanned link or a typed code. */
export async function pairPhone(
  page: Page,
  computer: Computer,
  how: How,
  name: string,
): Promise<{ words: string[]; offer: { link: string; typedCode: string } }> {
  const d = computer.driver;
  const offer = (await d.call("offer")) as { pairingId: string; link: string; typedCode: string };
  if (how === "scanned") {
    await page.goto(offer.link);
  } else {
    await page.goto(`https://${HOST}/p`);
    await expect(
      page.getByRole("heading", { name: "Type the code from your computer" }),
    ).toBeVisible();
    const groups = offer.typedCode.replace(/[^0-9A-Za-z]/g, "").match(/.{6}/g) ?? [];
    for (const [i, g] of groups.entries()) await page.getByLabel(`Group ${i + 1}`).fill(g);
    await page.getByRole("button", { name: "Continue" }).click();
  }
  await expect(page.getByRole("heading", { name: "Name this phone" })).toBeVisible();
  await page.getByLabel("Name").fill(name);
  await page.getByRole("button", { name: "Connect" }).click();
  const asked = (await d.call("request")) as {
    pairingId: string;
    deviceName: string;
    words: string[];
  };
  expect(asked.deviceName).toBe(name);
  await expect(
    page.getByRole("list", { name: `Check words: ${asked.words.join(", ")}` }),
  ).toBeVisible();
  await d.call("decide", { pairingId: asked.pairingId, allow: true });
  await expect(page.getByRole("heading", { name: `${name} can now use Nocturne.` })).toBeVisible();
  await page.getByRole("button", { name: "Not now, open Nocturne" }).click();
  await expect(page.getByText(/^Connected to /)).toBeVisible();
  return { words: asked.words, offer };
}
