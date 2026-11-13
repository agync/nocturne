// The owner-approved real-network trials of Away from home (plan 12, D10): the computer's side
// with the real helper (cloudflared from its pin, or the tailnet helper), and Chromium as the
// phone at the helper's real public address. Never part of a normal run: each trial reaches
// Cloudflare or Tailscale. Results go to TRIALS_DIR as JSON lines; console output masks the
// address.

import { Resolver } from "node:dns/promises";
import {
  appendFileSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  statSync,
} from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect } from "../e2e-away/fixtures";
import { Driver } from "../interop/driver";

export const TRIALS = process.env.TRIALS_DIR ?? "";
export type Connection = "simple" | "mostPrivate";
export const CONNECTION = (process.env.TRIAL_CONNECTION ?? "simple") as Connection;

export function record(file: string, entry: Record<string, unknown>): void {
  if (!TRIALS) throw new Error("TRIALS_DIR is not set");
  mkdirSync(TRIALS, { recursive: true });
  appendFileSync(
    join(TRIALS, file),
    `${JSON.stringify({ at: new Date().toISOString(), ...entry })}\n`,
  );
}

/** An address for the console: the first two letters of its name, never the whole of it. */
export function mask(host: string): string {
  return `${host.slice(0, 2)}…${host.slice(host.indexOf("."))}`;
}

export const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

export interface Real {
  driver: Driver;
  host: string;
  data: string;
}

export async function startReal(
  connection: Connection = CONNECTION,
  opts: { wrap?: string[]; on?: boolean } = {},
): Promise<Real> {
  const env: Record<string, string> = { AWAY_REAL: connection };
  if (connection === "mostPrivate") {
    const helper = process.env.AWAY_REAL_TAILNET;
    if (!helper) throw new Error("AWAY_REAL_TAILNET names the locally built tailnet helper");
    env.AWAY_REAL_TAILNET = helper;
  }
  const driver = await Driver.start(env, opts);
  return { driver, host: driver.info.host, data: driver.info.dataDir };
}

/** Wait for the helper to be up and return its address. */
export async function hostOf(real: Real, secs = 120): Promise<string> {
  real.host = String(await real.driver.call("wait_on", { secs }));
  return real.host;
}

/** Processes whose program lives under this data folder's helpers. */
export function helperPids(data: string): Array<{ pid: number; exe: string }> {
  const under = join(data, "remote", "helpers");
  const found: Array<{ pid: number; exe: string }> = [];
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const exe = readlinkSync(`/proc/${entry}/exe`);
      if (exe.startsWith(`${under}/`)) found.push({ pid: Number(entry), exe });
    } catch {
      // Gone, or not ours.
    }
  }
  return found;
}

export function rssKb(pid: number): number {
  try {
    const m = /VmRSS:\s+(\d+) kB/.exec(readFileSync(`/proc/${pid}/status`, "utf8"));
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}

export function hwmKb(pid: number): number {
  try {
    const m = /VmHWM:\s+(\d+) kB/.exec(readFileSync(`/proc/${pid}/status`, "utf8"));
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}

/** Partial uploads waiting under the data folder, with their sizes. */
export function partials(data: string): Array<{ name: string; size: number }> {
  const dir = join(data, "remote", "incoming");
  try {
    return readdirSync(dir)
      .filter((f) => f.endsWith(".part"))
      .map((f) => {
        try {
          return { name: f, size: statSync(join(dir, f)).size };
        } catch {
          return { name: f, size: -1 };
        }
      });
  } catch {
    return [];
  }
}

/** Every visible text on the page, for a private word that must never show. */
export async function pageText(page: Page): Promise<string> {
  // biome-ignore lint/performance/useDomNodeTextContent: only the text a person would see counts
  return page.evaluate(() => document.body.innerText);
}

export async function pairScanned(page: Page, real: Real, name: string): Promise<void> {
  const offer = (await real.driver.call("offer")) as { link: string };
  await page.goto(offer.link);
  await finishPairing(page, real, name);
}

export async function pairTyped(page: Page, real: Real, name: string): Promise<void> {
  const offer = (await real.driver.call("offer")) as { typedCode: string };
  await page.goto(`https://${real.host}/p`);
  await expect(page.getByRole("heading", { name: "Type the code from your computer" })).toBeVisible(
    {
      timeout: 30_000,
    },
  );
  const groups = offer.typedCode.replace(/[^0-9A-Za-z]/g, "").match(/.{6}/g) ?? [];
  for (const [i, g] of groups.entries()) await page.getByLabel(`Group ${i + 1}`).fill(g);
  await page.getByRole("button", { name: "Continue" }).click();
  await finishPairing(page, real, name);
}

async function finishPairing(page: Page, real: Real, name: string): Promise<void> {
  const d = real.driver;
  await expect(page.getByRole("heading", { name: "Name this phone" })).toBeVisible({
    timeout: 30_000,
  });
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
  await expect(page.getByRole("heading", { name: `${name} can now use Nocturne.` })).toBeVisible({
    timeout: 30_000,
  });
  await page.getByRole("button", { name: "Not now, open Nocturne" }).click();
  await expect(page.getByText(/^Connected to /)).toBeVisible({ timeout: 30_000 });
}

export async function home(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: /^(Back to home|Home)$/ })
    .first()
    .click();
  await expect(page.getByText(/^Connected to /)).toBeVisible({ timeout: 30_000 });
}

export async function ask(page: Page, question: string): Promise<void> {
  await page.getByRole("button", { name: /^Ask/ }).click();
  await page.getByLabel("Your question").fill(question);
  await page.getByRole("button", { name: "Ask", exact: true }).click();
}

export async function deviceId(real: Real, name: string): Promise<string> {
  const devices = (await real.driver.call("devices")) as Array<{ id: string; name: string }>;
  const found = devices.find((x) => x.name === name);
  if (!found) throw new Error(`no phone named ${name}`);
  return found.id;
}

/**
 * Wait until a new quick tunnel's name resolves, asking Cloudflare's own resolver so this
 * computer's resolver never caches an early "no such name". Returns how long it took and how
 * many early answers said the name does not exist.
 */
export async function waitForName(
  host: string,
  secs = 180,
): Promise<{ ms: number; misses: number }> {
  if (!host.endsWith(".trycloudflare.com")) return { ms: 0, misses: 0 };
  const r = new Resolver();
  r.setServers(["1.1.1.1"]);
  const started = Date.now();
  let misses = 0;
  while (Date.now() - started < secs * 1000) {
    try {
      if ((await r.resolve4(host)).length > 0) return { ms: Date.now() - started, misses };
    } catch {
      misses++;
    }
    await sleep(1000);
  }
  throw new Error("the name never resolved");
}
