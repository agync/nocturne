// Trial checks 3, 4 (retired names), 5 and 6 through the real quick tunnel (Simple only): the
// metrics line at the pinned version, what old addresses serve, a hostile program on the usual
// metrics ports, and one bounded burst of strangers while a paired phone keeps working.

import { randomBytes } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { join } from "node:path";
import { test } from "@playwright/test";
import { expect } from "../e2e-away/fixtures";
import { instrument } from "../e2e-away/phone";
import {
  ask,
  helperPids,
  mask,
  pairScanned,
  type Real,
  record,
  sleep,
  startReal,
  TRIALS,
  waitForName,
} from "./trial";

const LOG = "simple-edge.jsonl";
const EVIL = "evil-trial-substitute.trycloudflare.com";
let real: Real;
const hostile: http.Server[] = [];
const hostileHits: string[] = [];

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.setTimeout(10 * 60_000);
  // Check 5: a program that got there first holds cloudflared's usual metrics ports and names
  // another address.
  for (let port = 20241; port <= 20245; port++) {
    const server = http.createServer((req, res) => {
      hostileHits.push(`${port} ${req.url}`);
      if (req.url === "/quicktunnel") res.end(JSON.stringify({ hostname: EVIL }));
      else res.end("ok");
    });
    await new Promise<void>((done) => {
      server.once("error", () => done());
      server.listen(port, "127.0.0.1", () => done());
    });
    if (server.listening) hostile.push(server);
  }
  real = await startReal("simple");
  real.host = String(await real.driver.call("wait_on", { secs: 300 }));
  const name = await waitForName(real.host);
  record(LOG, {
    step: "on",
    host: real.host,
    nameResolvedAfter: name,
    hostilePorts: hostile.length,
  });
  console.log(`up at ${mask(real.host)} with ${hostile.length} hostile ports held`);
});

test.afterAll(async () => {
  for (const s of hostile) s.close();
  await real.driver.stop();
});

test("check 5: a hostile local port holder cannot substitute the address", async () => {
  const r = await fetch(`https://${real.host}/p`);
  const served = await r.text();
  const local = readFileSync(join(process.cwd(), "dist", "phone", "index.html"), "utf8");
  record(LOG, {
    check: 5,
    hostilePortsHeld: hostile.length,
    hostileHits,
    addressIsEvil: real.host === EVIL,
    servesOurPage: served === local,
  });
  expect(hostile.length).toBe(5);
  expect(real.host).not.toBe(EVIL);
  expect(served).toBe(local);
});

test("check 3: the metrics line at the pinned version", async () => {
  const logs = join(real.data, "remote", "logs");
  const text = readdirSync(logs)
    .filter((f) => f.includes("cloudflared"))
    .map((f) => readFileSync(join(logs, f), "utf8"))
    .join("\n");
  const line = /Starting metrics server on (\S+)/.exec(text);
  const version = /INF Version (\S+)/.exec(text)?.[1];
  const port = line?.[1] ? Number(/:(\d+)\//.exec(line[1])?.[1]) : 0;
  const pid = helperPids(real.data)[0]?.pid ?? 0;
  const listens = readFileSync("/proc/net/tcp", "utf8")
    .split("\n")
    .some((l) => {
      const f = l.trim().split(/\s+/);
      return (
        f[1] === `0100007F:${port.toString(16).toUpperCase().padStart(4, "0")}` && f[3] === "0A"
      );
    });
  const ready = await fetch(`http://127.0.0.1:${port}/ready`).then(
    (r) => r.status,
    () => 0,
  );
  const qt = await fetch(`http://127.0.0.1:${port}/quicktunnel`).then(
    (r) => r.text(),
    () => "",
  );
  record(LOG, {
    check: 3,
    line: line?.[0],
    version,
    loopbackListening: listens,
    ready,
    quicktunnelNamesOurHost: qt.includes(real.host),
    helperPid: pid,
    argv: pid
      ? readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0").filter(Boolean).slice(1)
      : [],
  });
  expect(line?.[1]).toMatch(/^127\.0\.0\.1:\d+\/metrics$/);
  expect(version).toBe("2026.11.0");
  expect(listens).toBe(true);
});

function helloFrame(): Buffer {
  const header = Buffer.alloc(26);
  header[0] = 1;
  header[1] = 1;
  randomBytes(16).copy(header, 2);
  const body = Buffer.alloc(4 + header.length);
  body.writeUInt32BE(header.length, 0);
  header.copy(body, 4);
  return body;
}

test("check 6: one bounded burst of strangers does not lock out a paired phone", async ({
  browser,
}) => {
  test.setTimeout(15 * 60_000);
  const context = await browser.newContext();
  const page = await context.newPage();
  await instrument(page, "ws");
  await pairScanned(page, real, "Burst trial");
  const timeAsk = async (label: string) => {
    const t = Date.now();
    try {
      await ask(page, "When is the boiler serviced?");
      await expect(page.getByText("The boiler is serviced every spring.").first()).toBeVisible({
        timeout: 45_000,
      });
      const ms = Date.now() - t;
      await page
        .getByRole("button", { name: /^(Back to home|Home)$/ })
        .first()
        .click();
      return { label, ok: true, ms };
    } catch (e) {
      const shown = await page.evaluate(() => document.body.textContent?.slice(0, 300) ?? "");
      await page.goto(`https://${real.host}/`).catch(() => {});
      return { label, ok: false, ms: Date.now() - t, shown, error: String(e).slice(0, 200) };
    }
  };
  const asks: Array<Record<string, unknown>> = [await timeAsk("before")];

  // 16 idle WebSockets that never say anything.
  const sockets: Array<{ opened: number; closed: number; code: number }> = [];
  const ws: WebSocket[] = [];
  const started = Date.now();
  for (let i = 0; i < 16; i++) {
    const entry = { opened: -1, closed: -1, code: 0 };
    sockets.push(entry);
    const s = new WebSocket(`wss://${real.host}/s`);
    s.onopen = () => {
      entry.opened = Date.now() - started;
    };
    s.onclose = (e) => {
      entry.closed = Date.now() - started;
      entry.code = e.code;
    };
    s.onerror = () => {};
    ws.push(s);
  }
  // 16 POSTs that promise a body and stall after ten bytes.
  const stalled: Array<{ status: number; ended: number; error: string }> = [];
  const requests: http.ClientRequest[] = [];
  for (let i = 0; i < 16; i++) {
    const entry = { status: 0, ended: -1, error: "" };
    stalled.push(entry);
    const req = https.request(
      {
        host: real.host,
        path: "/s",
        method: "POST",
        headers: { "content-type": "application/octet-stream", "content-length": 4096 },
        agent: false,
      },
      (res) => {
        entry.status = res.statusCode ?? 0;
        res.resume();
        res.on("end", () => {
          entry.ended = Date.now() - started;
        });
      },
    );
    req.on("error", (e) => {
      entry.error = e.message;
      entry.ended = Date.now() - started;
    });
    req.write(Buffer.alloc(10));
    requests.push(req);
  }
  await sleep(3000);
  asks.push(await timeAsk("during, held connections open"));
  // 50 pairing HELLOs for offers that do not exist, eight connections at a time.
  const agent = new https.Agent({ keepAlive: true, maxSockets: 8 });
  const hellos = await Promise.all(
    Array.from(
      { length: 50 },
      () =>
        new Promise<number>((done) => {
          const body = helloFrame();
          const req = https.request(
            {
              host: real.host,
              path: "/s",
              method: "POST",
              agent,
              headers: {
                "content-type": "application/octet-stream",
                "content-length": body.length,
              },
            },
            (res) => {
              res.resume();
              res.on("end", () => done(res.statusCode ?? 0));
            },
          );
          req.on("error", () => done(-1));
          req.end(body);
        }),
    ),
  );
  agent.destroy();
  const statusCounts: Record<string, number> = {};
  for (const s of hellos) statusCounts[s] = (statusCounts[s] ?? 0) + 1;
  asks.push(await timeAsk("after the hellos"));
  // A fresh session (the page reloaded) while the strangers still hold on.
  await page.reload();
  await expect(page.getByText(/^Connected to /))
    .toBeVisible({ timeout: 60_000 })
    .catch(() => {});
  asks.push(await timeAsk("after a reload, strangers still holding"));
  // Through Cloudflare's own timeouts: wait for the stalled bodies and sockets to end.
  const waitStart = Date.now();
  while (Date.now() - waitStart < 150_000) {
    if (stalled.every((s) => s.ended >= 0) && sockets.every((s) => s.closed >= 0)) break;
    await sleep(1000);
  }
  asks.push(await timeAsk("after the edge timed them out"));
  for (const r of requests) r.destroy();
  for (const s of ws) s.close();
  record(LOG, {
    check: 6,
    asks,
    idleSockets: sockets,
    stalledPosts: stalled,
    hellos: statusCounts,
    connections: { sockets: 16, stalled: 16, helloSockets: 8 },
  });
  console.log(`check 6 asks: ${JSON.stringify(asks.map((a) => [a.label, a.ok, a.ms]))}`);
  console.log(`hellos: ${JSON.stringify(statusCounts)}`);
  await context.close();
  expect(asks.every((a) => a.ok)).toBe(true);
});

test("check 4: what retired quick-tunnel names serve, and whether names repeat", async () => {
  const names = new Set<string>();
  const all: string[] = [];
  for (const f of readdirSync(TRIALS).filter((f) => f.endsWith(".jsonl"))) {
    for (const line of readFileSync(join(TRIALS, f), "utf8").split("\n")) {
      const m = /"host":"([a-z0-9-]+\.trycloudflare\.com)"/.exec(line);
      if (m?.[1]) {
        all.push(m[1]);
        names.add(m[1]);
      }
    }
  }
  try {
    for (const line of readFileSync(join(TRIALS, "cf-exp", "hosts.txt"), "utf8").split("\n")) {
      const h = line.split(" ")[1];
      if (h?.endsWith(".trycloudflare.com")) {
        all.push(h);
        names.add(h);
      }
    }
  } catch {
    // No experiments.
  }
  const retired = [...names].filter((h) => h !== real.host);
  const served: Array<Record<string, unknown>> = [];
  for (const h of retired) {
    try {
      const r = await fetch(`https://${h}/p`, { signal: AbortSignal.timeout(20_000) });
      const body = await r.text();
      served.push({
        host: h,
        status: r.status,
        server: r.headers.get("server"),
        ours: body.includes('rel="manifest"'),
        snippet: body.replace(/\s+/g, " ").slice(0, 160),
      });
    } catch (e) {
      served.push({ host: h, error: String((e as Error).cause ?? e).slice(0, 120) });
    }
  }
  record(LOG, {
    check: 4,
    namesSeen: names.size,
    starts: all.length,
    repeated: all.length - names.size,
    served,
  });
  console.log(
    `check 4: ${retired.length} retired names; answers ${JSON.stringify(served.map((s) => s.status ?? s.error))}`,
  );
  expect(served.filter((s) => s.ours === true)).toEqual([]);
});
