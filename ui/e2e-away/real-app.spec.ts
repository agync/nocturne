// I5 on Linux: the real desktop app (a debug build with the scripted mind and the stand-in
// helper) under a virtual display. Turn Away from home on, pair the Chromium phone, turn it off;
// nothing of the helper or its sockets may remain, quitting must leave no process, and the
// memory and timer budgets are measured and recorded.
//
// Needs AWAY_REAL_APP, the built app (see scripts/away-integration.sh real). Skipped otherwise.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "./fixtures";
import { pairPhone } from "./phone";
import { listeners, pidsRunning, RealApp, rssKb, treeRssKb } from "./realApp";
import type { Computer } from "./world";

const APP = process.env.AWAY_REAL_APP;
// AWAY_REAL_FAST=1 shortens every wait, to try the steps; the budgets are only judged at full length.
const FAST = process.env.AWAY_REAL_FAST === "1";
const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));
const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;

async function sample(pid: number, n = 6, gap = 1000): Promise<number> {
  const xs: number[] = [];
  for (let i = 0; i < n; i++) {
    xs.push(rssKb(pid));
    await sleep(gap);
  }
  return mean(xs);
}

/** Wake-ups of the app's async runtime threads that found nothing waiting, over a window. */
async function wakeups(
  pid: number,
  seconds: number,
): Promise<{ count: number; clusters: number[]; byThread: Record<string, number> }> {
  const out = join(mkdtempSync(join(tmpdir(), "away-strace-")), "t.log");
  const began = Date.now() / 1000;
  const tracer = spawn(
    "strace",
    [
      "-f",
      "-ttt",
      "-T",
      "-e",
      "trace=epoll_wait,epoll_pwait,epoll_pwait2",
      "-o",
      out,
      "-p",
      String(pid),
    ],
    { stdio: "ignore" },
  );
  await sleep(seconds * 1000);
  const names = new Map<number, string>();
  for (const tid of readdirSync(`/proc/${pid}/task`)) {
    try {
      names.set(Number(tid), readFileSync(`/proc/${pid}/task/${tid}/comm`, "utf8").trim());
    } catch {
      // Gone.
    }
  }
  tracer.kill("SIGINT");
  await new Promise((done) => tracer.on("exit", done));
  const times: number[] = [];
  let last: number | undefined;
  const byThread: Record<string, number> = {};
  for (const line of readFileSync(out, "utf8").split("\n")) {
    const done = /^(\d+)\s+(\d+\.\d+) epoll_\w+\(.*\) = 0 <(\d+\.\d+)>/.exec(line);
    const resumed = /^(\d+)\s+(\d+\.\d+) <\.\.\. epoll_\w+ resumed>.*\) = 0 <(\d+\.\d+)>/.exec(
      line,
    );
    const m = done ?? resumed;
    if (!m) continue;
    const tid = Number(m[1]);
    const name = names.get(tid) ?? "gone";
    byThread[name] = (byThread[name] ?? 0) + 1;
    if (!name.startsWith("tokio")) continue;
    times.push(Number(m[2]) + (done ? Number(m[3]) : 0));
  }
  times.sort((a, b) => a - b);
  // Several threads wake together for one timer: wake-ups within two seconds are one event.
  const clusters: number[] = [];
  for (const t of times) {
    if (clusters.length === 0 || t - (last ?? 0) > 2)
      clusters.push(Math.round((t - began) * 10) / 10);
    last = t;
  }
  return { count: times.length, clusters, byThread };
}

test.describe("the real app", () => {
  test.skip(!APP, "set AWAY_REAL_APP to the built app");
  test.setTimeout(600_000);

  test("turn on, pair, upload, turn off, quit: nothing left, budgets met", async ({
    browser,
    front,
  }) => {
    const app = new RealApp(APP as string);
    const report: Record<string, unknown> = {};
    try {
      await app.start();
      const pid = app.pid;
      await sleep(FAST ? 3000 : 15_000);

      // Away from home off: the baseline.
      const listenersOff = listeners(pid);
      const baseKb = await sample(pid);
      const baseTreeKb = treeRssKb(pid);
      const offWindow = FAST ? 8 : 60;
      const offWake = await wakeups(pid, offWindow);

      // Turn it on, no phone.
      await app.invoke("start_remote", { connection: "simple", allowDownload: false });
      await expect
        .poll(
          async () => ((await app.invoke("get_remote")) as { state: { type: string } }).state.type,
          {
            timeout: 30_000,
          },
        )
        .toBe("on");
      front.target = app.publicPort;
      expect(pidsRunning([app.helperDir]).length, "the helper runs").toBeGreaterThan(0);
      await sleep(FAST ? 2000 : 10_000);
      const onKb = await sample(pid);
      const onTreeKb = treeRssKb(pid);
      const onWindow = FAST ? 8 : 100;
      const onWake = await wakeups(pid, onWindow);
      report.idleGrowthMb = (onKb - baseKb) / 1024;
      report.baseMb = baseKb / 1024;
      report.onMb = onKb / 1024;
      report.treeMb = { off: baseTreeKb / 1024, on: onTreeKb / 1024 };
      report.wakeups = {
        offWindowSeconds: offWindow,
        offCount: offWake.count,
        offByThread: offWake.byThread,
        onWindowSeconds: onWindow,
        onCount: onWake.count,
        onByThread: onWake.byThread,
        onEventsAtSeconds: onWake.clusters,
        offEventsAtSeconds: offWake.clusters,
      };

      // Pair the Chromium phone through the app's own commands.
      const host = {
        driver: {
          call: async (cmd: string, a: Record<string, unknown> = {}) => {
            if (cmd === "offer") return app.invoke("begin_pairing");
            if (cmd === "decide")
              return app.invoke("decide_pairing", { pairingId: a.pairingId, allow: a.allow });
            let found: unknown;
            await expect
              .poll(
                async () => {
                  const view = (await app.invoke("get_remote")) as {
                    pairing?: { phase: { type: string; request?: unknown } };
                  };
                  found =
                    view.pairing?.phase.type === "waitingForYou"
                      ? view.pairing.phase.request
                      : undefined;
                  return found !== undefined;
                },
                { timeout: 30_000 },
              )
              .toBe(true);
            const r = found as { pairingId: string; deviceName: string; checkPhrase: string[] };
            return { pairingId: r.pairingId, deviceName: r.deviceName, words: r.checkPhrase };
          },
        },
      } as unknown as Computer;
      const context = await browser.newContext();
      const page = await context.newPage();
      await pairPhone(page, host, "scanned", "Pixel");
      await page.getByRole("button", { name: /^Keep/ }).click();
      await page.getByLabel("Type something to keep").fill("Real app note");
      await page.getByRole("button", { name: "Keep", exact: true }).click();
      await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
      await page.getByRole("button", { name: "Keep another" }).click();

      // A 50 MB upload, with the app's memory sampled every 50 ms.
      const file = join(app.scratch, "big.jpg");
      const big = Buffer.alloc(50_000_000, 7);
      Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46, 0x49, 0x46]).copy(big);
      writeFileSync(file, big);
      await sleep(3000);
      const before = await sample(pid, 4, 500);
      let peak = 0;
      const timer = setInterval(() => {
        peak = Math.max(peak, rssKb(pid));
      }, 50);
      await page.locator('input[type="file"]').first().setInputFiles(file);
      await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
      await page.getByRole("button", { name: "Keep", exact: true }).click();
      await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible({ timeout: 180_000 });
      await sleep(2000);
      clearInterval(timer);
      report.uploadMb = 50_000_000 / 1_000_000;
      report.uploadPeakGrowthMb = (peak - before) / 1024;
      report.uploadBeforeMb = before / 1024;
      report.uploadPeakMb = peak / 1024;
      await context.close();

      // Turn off: nothing of the helper or its sockets remains.
      await app.invoke("stop_remote", { removeAll: false });
      await expect
        .poll(
          async () => ((await app.invoke("get_remote")) as { state: { type: string } }).state.type,
          {
            timeout: 15_000,
          },
        )
        .toBe("off");
      await sleep(1500);
      report.helperProcessesAfterOff = pidsRunning([app.helperDir]).map((p) => p.pid);
      report.listenersBefore = listenersOff;
      report.listenersAfterOff = listeners(pid);
      report.listenerFileAfterOff = existsSync(join(app.data, "remote", "run", "listener.sock"));
      expect(report.helperProcessesAfterOff).toEqual([]);
      expect(report.listenersAfterOff).toEqual(listenersOff);
      expect(report.listenerFileAfterOff).toBe(false);

      // Quit: no engine and no helper is left.
      const quitMs = await app.quit();
      const logs = app.logs;
      report.quitMs = quitMs;
      report.leftAfterQuit = pidsRunning([app.binary, app.helperDir, join(app.scratch)]).map(
        (p) => p.exe,
      );
      report.lemondLeft = pidsRunning(["/"]).filter((p) => p.exe.endsWith("/lemond")).length;
      report.quitLogged = logs.includes("quit: everything stopped");
      expect(report.leftAfterQuit).toEqual([]);
      expect(report.quitLogged).toBe(true);

      // The budgets. Wake-ups of the async runtime come in groups (a timer and the threads it
      // wakes); groups further apart than five seconds are separate timers, and none repeats
      // more often than about every thirty seconds (housekeeping and log rotation, each 30 s,
      // so their groups interleave).
      const onEvents = onWake.clusters;
      const repeats = onEvents
        .slice(1)
        .map((t, i) => t - (onEvents[i] ?? t))
        .filter((g) => g >= 5);
      report.fastestRepeatSeconds = Math.min(...repeats, Number.POSITIVE_INFINITY);
      expect(report.fastestRepeatSeconds as number).toBeGreaterThanOrEqual(15);
      expect(report.idleGrowthMb as number).toBeLessThan(25);
      expect(report.uploadPeakGrowthMb as number).toBeLessThan(10);
    } finally {
      writeFileSync(
        join(process.cwd(), "test-results", "away", "i5-report.json"),
        JSON.stringify(report, null, 2),
      );
      console.log(`I5 ${JSON.stringify(report)}`);
      await app.close();
    }
  });
});
