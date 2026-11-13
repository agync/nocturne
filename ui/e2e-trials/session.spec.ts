// Trial checks 1, 2, 7 and 8 through the real helper (TRIAL_CONNECTION=simple|mostPrivate): pair
// and use a phone, compare what the phone receives with the built phone app, keep a private
// memory at home, and remove phones in the middle of an answer and of an upload, over a
// WebSocket and over long-poll.

import { execFileSync } from "node:child_process";
import { Resolver } from "node:dns/promises";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Browser, type Page, test } from "@playwright/test";
import { expect } from "../e2e-away/fixtures";
import { instrument, type Mode, netEvents, photo } from "../e2e-away/phone";
import {
  ask,
  CONNECTION,
  deviceId,
  home,
  mask,
  pageText,
  pairScanned,
  pairTyped,
  partials,
  type Real,
  record,
  sleep,
  startReal,
  TRIALS,
  waitForName,
} from "./trial";

const LOG = `${CONNECTION}-session.jsonl`;
let real: Real;

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.setTimeout(30 * 60_000);
  const trace = process.env.TRIAL_STRACE;
  const wrap = trace
    ? [
        "strace",
        "-f",
        "-qq",
        "-e",
        "trace=%network,execve,clone,clone3,fork,vfork",
        "-s",
        "256",
        "-o",
        trace,
      ]
    : undefined;
  real = await startReal(CONNECTION, { wrap });
  if (CONNECTION === "mostPrivate") {
    // Hand every sign-in step to the owner through the trials folder; never open it here.
    const started = Date.now();
    let written = "";
    while (Date.now() - started < 20 * 60_000) {
      const state = (await real.driver.call("remote_state")) as {
        type: string;
        url?: string;
        step?: string;
      };
      if (state.type === "on") break;
      if (state.type === "needsSignIn" && state.url && state.url !== written) {
        writeFileSync(
          join(TRIALS, "TAILNET-LOGIN.txt"),
          `Step: ${state.step}\nOpen this link and sign in to your Tailscale account (or approve the device):\n${state.url}\n`,
          { mode: 0o600 },
        );
        written = state.url;
        record(LOG, { step: "sign-in link written", signInStep: state.step });
      }
      await sleep(2000);
    }
  }
  real.host = String(await real.driver.call("wait_on", { secs: 1200 }));
  const name = await waitForName(real.host);
  record(LOG, { step: "on", host: real.host, nameResolvedAfter: name });
  console.log(`up at ${mask(real.host)}`);
});

test.afterAll(async () => {
  await real.driver.stop();
});

function distFiles(): string[] {
  const root = join(process.cwd(), "dist", "phone");
  const walk = (dir: string, prefix: string): string[] =>
    readdirSync(join(root, dir), { withFileTypes: true }).flatMap((e) =>
      e.isDirectory() ? walk(join(dir, e.name), `${prefix}${e.name}/`) : [`${prefix}${e.name}`],
    );
  return walk("", "");
}

function expectedHeaders(): Record<string, string> {
  const pattern =
    CONNECTION === "simple"
      ? "https://*.trycloudflare.com wss://*.trycloudflare.com"
      : "https://*.ts.net wss://*.ts.net";
  return {
    "content-security-policy": `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; media-src blob:; font-src 'self'; connect-src 'self' ${pattern}; worker-src 'self'; manifest-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`,
    "referrer-policy": "no-referrer",
    "x-content-type-options": "nosniff",
    "cross-origin-opener-policy": "same-origin",
    "permissions-policy": "camera=(self), microphone=(self), geolocation=()",
    "cache-control": "no-cache",
  };
}

test("check 2: the phone receives exactly the built phone app", async ({ browser }) => {
  const root = join(process.cwd(), "dist", "phone");
  const names = distFiles().filter((n) => !n.endsWith(".map"));
  const paths = ["/", "/p", "/r", ...names.map((n) => `/${n}`)];
  const want = expectedHeaders();
  const results: Array<Record<string, unknown>> = [];
  const extraHeaders = new Map<string, string>();
  let mismatches = 0;
  for (const path of paths) {
    const file = ["/", "/p", "/r"].includes(path) ? "index.html" : path.slice(1);
    const local = readFileSync(join(root, file));
    const r = await fetch(`https://${real.host}${path}`, {
      headers: { "accept-encoding": "identity" },
    });
    const body = Buffer.from(await r.arrayBuffer());
    const same = r.status === 200 && Buffer.compare(body, local) === 0;
    const headerDiffs: string[] = [];
    for (const [k, v] of Object.entries(want)) {
      if (r.headers.get(k) !== v) headerDiffs.push(`${k}: ${r.headers.get(k)}`);
    }
    for (const [k, v] of r.headers) {
      if (!(k in want) && !["content-type", "content-length", "date"].includes(k))
        extraHeaders.set(k, k === "cf-ray" ? "(varies)" : v);
    }
    if (!same || headerDiffs.length) mismatches++;
    results.push({
      path,
      status: r.status,
      bytes: body.length,
      local: local.length,
      same,
      cdnCgi: body.toString("latin1").includes("/cdn-cgi/"),
      headerDiffs,
      contentType: r.headers.get("content-type"),
    });
  }
  // A HEAD and a gzip request, to see whether the edge rewrites what it compresses.
  const gz = await fetch(`https://${real.host}/p`, { headers: { "accept-encoding": "gzip, br" } });
  const gzBody = Buffer.from(await gz.arrayBuffer());
  record(LOG, {
    check: 2,
    via: "node fetch",
    results,
    mismatches,
    extraHeaders: Object.fromEntries(extraHeaders),
    compressed: {
      encoding: gz.headers.get("content-encoding"),
      sameAfterDecoding: Buffer.compare(gzBody, readFileSync(join(root, "index.html"))) === 0,
    },
  });

  // The same through Chromium: every response the phone page gets, and every host it asks.
  const context = await browser.newContext();
  const page = await context.newPage();
  const seen: Array<{ url: string; status: number; same: boolean | null }> = [];
  const work: Promise<void>[] = [];
  page.on("response", (r) => {
    work.push(
      (async () => {
        const u = new URL(r.url());
        let same: boolean | null = null;
        const file = ["/", "/p", "/r"].includes(u.pathname) ? "index.html" : u.pathname.slice(1);
        if (
          u.host === real.host &&
          r.request().method() === "GET" &&
          names.concat("index.html").includes(file)
        ) {
          try {
            same = Buffer.compare(await r.body(), readFileSync(join(root, file))) === 0;
          } catch {
            same = null;
          }
        }
        seen.push({
          url: u.host === real.host ? u.pathname : `OTHER:${u.host}${u.pathname}`,
          status: r.status(),
          same,
        });
      })(),
    );
  });
  await page.goto(`https://${real.host}/p`);
  await expect(page.getByRole("heading", { name: "Type the code from your computer" })).toBeVisible(
    {
      timeout: 30_000,
    },
  );
  await sleep(3000);
  await Promise.all(work);
  const html = await page.content();
  const sw = await page.evaluate(async () =>
    (await navigator.serviceWorker?.getRegistrations())?.map(
      (r) => r.active?.scriptURL ?? r.installing?.scriptURL ?? "",
    ),
  );
  record(LOG, {
    check: 2,
    via: "chromium",
    responses: seen,
    otherHosts: seen.filter((s) => s.url.startsWith("OTHER:")).length,
    cdnCgi: seen.some((s) => s.url.includes("/cdn-cgi/")) || html.includes("/cdn-cgi/"),
    scriptsInDom: await page.evaluate(() =>
      [...document.scripts].map((s) => s.src.replace(location.origin, "")),
    ),
    serviceWorkers: (sw ?? []).map((u) => u.replace(`https://${real.host}`, "")),
  });
  await context.close();
  expect(mismatches, "every file and header as built").toBe(0);
  expect(seen.filter((s) => s.url.startsWith("OTHER:") || s.url.includes("/cdn-cgi/"))).toEqual([]);
  expect(seen.filter((s) => s.same === false)).toEqual([]);
});

async function newPhone(
  browser: Browser,
  mode: Mode,
): Promise<{ page: Page; errors: string[]; close: () => Promise<void> }> {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await instrument(page, mode);
  return { page, errors, close: () => context.close() };
}

/** Remove the phone while an answer is held open; how long until each part stopped. */
async function removeMidAnswer(
  page: Page,
  name: string,
  mode: Mode,
): Promise<Record<string, number>> {
  const d = real.driver;
  const id = await deviceId(real, name);
  await d.call("gate", { open: false });
  await ask(page, "When is the dentist?");
  await d.call("entered");
  await expect.poll(() => d.call("inflight", { device: id }), { timeout: 30_000 }).toBe(1);
  const removedAt = Date.now();
  await d.call("remove", { device: id });
  let answerStopped = -1;
  while (Date.now() - removedAt < 5000) {
    if ((await d.call("inflight", { device: id })) === 0) {
      answerStopped = Date.now() - removedAt;
      break;
    }
  }
  await expect(page.getByText("This phone was removed from Nocturne.")).toBeVisible({
    timeout: 10_000,
  });
  const screen = Date.now() - removedAt;
  await d.call("gate", { open: true });
  const ended = (await netEvents(page)).filter(
    (e) => e.kind === (mode === "ws" ? "ws-close" : "poll-end") && e.t >= removedAt,
  );
  return { answerStopped, heldEnded: (ended[0]?.t ?? Number.NaN) - removedAt, screen };
}

/** Remove the phone while a large photo is on its way; how long until each part stopped. */
async function removeMidUpload(
  page: Page,
  name: string,
  mode: Mode,
): Promise<Record<string, number>> {
  const d = real.driver;
  const id = await deviceId(real, name);
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({
      name: "big.jpg",
      mimeType: "image/jpeg",
      buffer: photo("UPLOAD-MARK-77", 45_000_000),
    });
  await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  const started = Date.now();
  let partSize = 0;
  while (Date.now() - started < 120_000) {
    partSize = partials(real.data).reduce((s, p) => s + Math.max(p.size, 0), 0);
    if (partSize > 2_000_000) break;
    await sleep(50);
  }
  expect(partSize, "the upload was under way").toBeGreaterThan(2_000_000);
  const removedAt = Date.now();
  await d.call("remove", { device: id });
  let uploadStopped = -1;
  let last = partSize;
  let stableSince = Date.now();
  while (Date.now() - removedAt < 10_000) {
    const now = partials(real.data);
    const size = now.reduce((s, p) => s + Math.max(p.size, 0), 0);
    if (now.length === 0) {
      uploadStopped = Date.now() - removedAt;
      break;
    }
    if (size !== last) {
      last = size;
      stableSince = Date.now();
    } else if (Date.now() - stableSince > 1500) {
      uploadStopped = stableSince - removedAt;
      break;
    }
    await sleep(20);
  }
  await expect(page.getByText("This phone was removed from Nocturne.")).toBeVisible({
    timeout: 10_000,
  });
  const screen = Date.now() - removedAt;
  const ended = (await netEvents(page)).filter(
    (e) => e.kind === (mode === "ws" ? "ws-close" : "poll-end") && e.t >= removedAt,
  );
  const left = partials(real.data);
  const items = (await d.call("items")) as Array<{ originalPath: string | null }>;
  const kept = items.filter(
    (i) => i.originalPath && readFileSync(i.originalPath).includes("UPLOAD-MARK-77"),
  );
  return {
    partBytesAtRemoval: partSize,
    uploadStopped,
    heldEnded: (ended[0]?.t ?? Number.NaN) - removedAt,
    screen,
    partsLeft: left.length,
    keptAnyway: kept.length,
  };
}

test("check 1 and 8 (WebSocket, mid-answer): pair, keep, ask, original, page, take back, remove", async ({
  browser,
}) => {
  test.setTimeout(10 * 60_000);
  const { page, errors, close } = await newPhone(browser, "ws");
  const d = real.driver;
  const steps: Record<string, unknown> = {};
  let t = Date.now();
  await pairScanned(page, real, "Trial phone");
  steps.pairMs = Date.now() - t;

  t = Date.now();
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page
    .getByLabel("Type something to keep")
    .fill("Parcel for TRIAL-TEXT-MARK is behind the shed");
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible({ timeout: 30_000 });
  const items = async () =>
    (await d.call("items")) as Array<{ text: string | null; originalPath: string | null }>;
  expect((await items()).filter((i) => i.text?.includes("TRIAL-TEXT-MARK"))).toHaveLength(1);
  steps.keepTextMs = Date.now() - t;

  t = Date.now();
  await page.getByRole("button", { name: "Keep another" }).click();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({
      name: "shed.jpg",
      mimeType: "image/jpeg",
      buffer: photo("TRIAL-PHOTO-MARK", 700_000),
    });
  await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible({ timeout: 60_000 });
  expect(
    (await items()).filter(
      (i) => i.originalPath && readFileSync(i.originalPath).includes("TRIAL-PHOTO-MARK"),
    ),
  ).toHaveLength(1);
  steps.keepPhotoMs = Date.now() - t;
  await home(page);

  t = Date.now();
  await ask(page, "When is the boiler serviced?");
  await expect(page.getByText("The boiler is serviced every spring.")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText(/^From \d things? you kept/).first()).toBeVisible();
  steps.askMs = Date.now() - t;

  t = Date.now();
  await page
    .getByRole("list", { name: /^From \d things? you kept/ })
    .getByRole("button")
    .first()
    .click();
  await expect(page.getByRole("img").first()).toBeVisible({ timeout: 30_000 });
  steps.originalMs = Date.now() - t;
  await page.getByRole("button", { name: "Answer" }).click();
  await home(page);

  t = Date.now();
  await page.getByRole("button", { name: /^Today’s page/ }).click();
  await expect(page.getByText("Phone edition · private lines left out")).toBeVisible({
    timeout: 30_000,
  });
  steps.pageMs = Date.now() - t;
  await home(page);

  t = Date.now();
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page.getByLabel("Type something to keep").fill("Note TRIAL-TAKE-BACK");
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "Take back" }).click();
  await expect(page.getByText("Taken back. It’s no longer kept.")).toBeVisible({ timeout: 30_000 });
  expect((await items()).filter((i) => i.text?.includes("TRIAL-TAKE-BACK"))).toHaveLength(0);
  steps.takeBackMs = Date.now() - t;
  await home(page);

  const removal = await removeMidAnswer(page, "Trial phone", "ws");
  const transport = (await netEvents(page)).some((e) => e.kind === "ws-open") ? "ws" : "poll";
  record(LOG, { check: "1+8", mode: "ws", mid: "answer", transport, steps, removal, errors });
  console.log(`check 1/8 ws answer: ${JSON.stringify({ steps, removal })}`);
  await page.screenshot({ path: `test-results/trials/${CONNECTION}-removed-ws-answer.png` });
  await close();
  expect(removal.answerStopped).toBeGreaterThanOrEqual(0);
  expect(removal.answerStopped).toBeLessThan(1000);
  expect(removal.heldEnded).toBeLessThan(1000);
  expect(errors).toEqual([]);
});

test("check 7: a note with one private and one public memory; private stays home", async ({
  browser,
}) => {
  test.setTimeout(10 * 60_000);
  const seeded = (await real.driver.call("seed_private")) as Record<string, string>;
  const { page, errors, close } = await newPhone(browser, "poll");
  await pairTyped(page, real, "Private trial");
  const shown: Record<string, string> = {};
  // The question itself names the spare key; only the answer, the word heron, is private.
  const words = ["heron"];
  const leaks = (text: string) => words.filter((w) => text.includes(w));

  // The engine withholds every memory of a note that holds anything private (its receipt and
  // original would carry the private sentence), so either answer is recorded.
  await ask(page, "When is lunch with Maya?");
  await expect(
    page
      .getByText("Lunch with Maya is on Friday at noon.")
      .or(page.getByText("Nothing you’ve kept answers that.")),
  ).toBeVisible({ timeout: 30_000 });
  await sleep(1500);
  shown.publicAnswer = await pageText(page);
  const sources = page.getByRole("list", { name: /^From \d things? you kept/ }).getByRole("button");
  if ((await sources.count()) > 0) {
    await sources.first().click();
    await sleep(4000);
    shown.original = await pageText(page);
    const back = page.getByRole("button", { name: "Answer" });
    if (await back.isVisible()) await back.click();
  } else {
    shown.original = "(no source button)";
  }
  await home(page);

  await ask(page, "What is the spare key hint?");
  await sleep(6000);
  shown.privateAnswer = await pageText(page);
  await home(page);

  await page.getByRole("button", { name: /^Today’s page/ }).click();
  await expect(page.getByText("Phone edition · private lines left out")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText("Opening today’s page…")).toBeHidden({ timeout: 30_000 });
  shown.page = await pageText(page);
  await home(page);

  const found = Object.fromEntries(Object.entries(shown).map(([k, v]) => [k, leaks(v)]));
  record(LOG, { check: 7, seeded: Object.keys(seeded), shown, leaks: found, errors });
  console.log(`check 7 leaks: ${JSON.stringify(found)}`);

  // While this phone is paired: remove it in the middle of an upload over long-poll (check 8).
  const removal = await removeMidUpload(page, "Private trial", "poll");
  record(LOG, { check: 8, mode: "poll", mid: "upload", removal, errors });
  console.log(`check 8 poll upload: ${JSON.stringify(removal)}`);
  await close();
  for (const [k, v] of Object.entries(found)) expect(v, k).toEqual([]);
  expect(removal.heldEnded).toBeLessThan(1000);
  expect(removal.uploadStopped).toBeGreaterThanOrEqual(0);
  expect(removal.uploadStopped).toBeLessThan(1000);
});

test("check 8: remove mid-answer over long-poll", async ({ browser }) => {
  test.setTimeout(10 * 60_000);
  const { page, errors, close } = await newPhone(browser, "poll");
  await pairScanned(page, real, "Poll trial");
  const removal = await removeMidAnswer(page, "Poll trial", "poll");
  const sockets = (await netEvents(page)).filter((e) => e.kind === "ws-open").length;
  record(LOG, { check: 8, mode: "poll", mid: "answer", removal, socketsOpened: sockets, errors });
  console.log(`check 8 poll answer: ${JSON.stringify(removal)}`);
  await close();
  expect(sockets).toBe(0);
  expect(removal.answerStopped).toBeGreaterThanOrEqual(0);
  expect(removal.answerStopped).toBeLessThan(1000);
  expect(removal.heldEnded).toBeLessThan(1000);
});

test("check 8: remove mid-upload over WebSocket", async ({ browser }) => {
  test.setTimeout(10 * 60_000);
  const { page, errors, close } = await newPhone(browser, "ws");
  await pairScanned(page, real, "Socket trial");
  const removal = await removeMidUpload(page, "Socket trial", "ws");
  record(LOG, { check: 8, mode: "ws", mid: "upload", removal, errors });
  console.log(`check 8 ws upload: ${JSON.stringify(removal)}`);
  await close();
  expect(removal.heldEnded).toBeLessThan(1000);
  expect(removal.uploadStopped).toBeGreaterThanOrEqual(0);
  expect(removal.uploadStopped).toBeLessThan(1000);
});

/** This computer's own Tailscale view of the trial node, by its machine name (read only). */
function peerView(host: string): Record<string, unknown> | null {
  const name = host.split(".")[0] ?? "";
  try {
    const status = JSON.parse(
      execFileSync("tailscale", ["status", "--json"], { encoding: "utf8" }),
    ) as {
      Peer?: Record<string, Record<string, unknown>>;
    };
    const peer = Object.values(status.Peer ?? {}).find(
      (p) => String(p.HostName ?? "") === name || String(p.DNSName ?? "").startsWith(`${name}.`),
    );
    if (!peer) return null;
    const keep = [
      "Online",
      "Active",
      "Tags",
      "ExitNode",
      "KeyExpiry",
      "Expired",
      "sshHostKeys",
      "PeerAPIURL",
      "CapMap",
      "Capabilities",
    ];
    return Object.fromEntries(Object.entries(peer).filter(([k]) => keep.includes(k)));
  } catch (e) {
    return { error: String(e).slice(0, 200) };
  }
}

test("most private: no Funnel, and what is left after signing out", async () => {
  test.skip(CONNECTION !== "mostPrivate", "Most private only");
  test.setTimeout(10 * 60_000);
  const r = new Resolver();
  r.setServers(["1.1.1.1"]);
  let publicDns: string;
  try {
    publicDns = (await r.resolve4(real.host)).join(",");
  } catch (e) {
    publicDns = `none (${(e as { code?: string }).code ?? "error"})`;
  }
  const before = peerView(real.host);
  record(LOG, { check: "funnel", publicDns, peerBefore: before });
  console.log(`public name: ${publicDns}`);
  const signedOut = await real.driver.call("sign_out");
  const after: Array<Record<string, unknown>> = [];
  for (const wait of [5, 30, 120]) {
    await sleep(wait * 1000);
    after.push({ afterSecs: wait, peer: peerView(real.host) });
  }
  const dir = join(real.data, "remote", "tailscale");
  let stateLeft: string[] = [];
  try {
    stateLeft = readdirSync(dir, { recursive: true }).map(String);
  } catch {
    stateLeft = ["(no state folder)"];
  }
  record(LOG, { check: "sign out", state: signedOut, peerAfter: after, stateLeft });
  console.log(
    `after sign out: ${JSON.stringify(after.map((a) => (a.peer === null ? "gone" : "listed")))}, state files left ${stateLeft.length}`,
  );
  expect(publicDns.startsWith("none")).toBe(true);
});
