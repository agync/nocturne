// Trial check 9 and the idle run (Simple): memory of the app and the helper while off, on and
// idle for ten minutes, with a paired phone idle for TRIAL_IDLE_MINUTES (default 60) with an ask
// every ten minutes, a frozen helper (the edge loses it, then it comes back), a killed helper
// (a new address, the phone re-links), what the old address serves, and a 50 MB upload.

import { test } from "@playwright/test";
import { expect } from "../e2e-away/fixtures";
import { instrument, netEvents, photo } from "../e2e-away/phone";
import {
  ask,
  helperPids,
  home,
  hwmKb,
  mask,
  pairScanned,
  partials,
  type Real,
  record,
  rssKb,
  sleep,
  startReal,
  waitForName,
} from "./trial";

const LOG = "simple-idle.jsonl";
const IDLE_MINUTES = Number(process.env.TRIAL_IDLE_MINUTES ?? 60);
/** Thirty-second samples with Away from home on and no phone (20 is ten minutes). */
const ON_SAMPLES = Number(process.env.TRIAL_ON_SAMPLES ?? 20);
let real: Real;

test.afterAll(async () => {
  await real?.driver.stop();
});

function sample(label: string): Record<string, unknown> {
  const app = real.driver.pid ?? 0;
  const helper = helperPids(real.data)[0]?.pid ?? 0;
  return {
    label,
    appKb: rssKb(app),
    appPeakKb: hwmKb(app),
    helperKb: helper ? rssKb(helper) : null,
    helperPid: helper || null,
  };
}

async function state(): Promise<{ type: string; address?: string; pairingLink?: string }> {
  return (await real.driver.call("remote_state")) as {
    type: string;
    address?: string;
    pairingLink?: string;
  };
}

test("check 9 and the idle run", async ({ browser }) => {
  test.setTimeout((IDLE_MINUTES + 60) * 60_000);
  real = await startReal("simple", { on: false });
  const samples: Array<Record<string, unknown>> = [];
  const keep = (label: string) => {
    const s = sample(label);
    samples.push({ t: new Date().toISOString(), ...s });
    record(LOG, { sample: s });
    return s;
  };

  // Off: the baseline.
  for (let i = 0; i < Math.min(4, ON_SAMPLES); i++) {
    keep("off");
    await sleep(30_000);
  }
  const off = keep("off");
  await real.driver.call("on");
  real.host = String(await real.driver.call("wait_on", { secs: 300 }));
  const name = await waitForName(real.host);
  record(LOG, { step: "on", host: real.host, nameResolvedAfter: name });
  console.log(`up at ${mask(real.host)}`);

  // On, no phone, ten minutes.
  const onStart = keep("on, no phone");
  for (let i = 0; i < ON_SAMPLES; i++) {
    await sleep(30_000);
    keep("on, no phone");
  }
  const onEnd = keep("on, no phone");
  record(LOG, {
    check: 9,
    phase: "on and idle 10 min",
    appGrowthKbVsOff: (onEnd.appKb as number) - (off.appKb as number),
    appGrowthKbOverTenMinutes: (onEnd.appKb as number) - (onStart.appKb as number),
    helperKb: onEnd.helperKb,
  });

  // A paired phone, idle, asking every ten minutes.
  const context = await browser.newContext();
  const page = await context.newPage();
  await instrument(page, "ws");
  await pairScanned(page, real, "Idle trial");
  const asks: Array<Record<string, unknown>> = [];
  const timedAsk = async (label: string) => {
    const t = Date.now();
    try {
      await ask(page, "When is the boiler serviced?");
      await expect(page.getByText("The boiler is serviced every spring.").first()).toBeVisible({
        timeout: 60_000,
      });
      await home(page);
      const r = { label, ok: true, ms: Date.now() - t };
      asks.push(r);
      record(LOG, { ask: r });
      return r;
    } catch (e) {
      const shown = await page
        .evaluate(() => document.body.textContent?.slice(0, 300) ?? "")
        .catch(() => "");
      await page
        .screenshot({ path: `test-results/trials/idle-${label.replace(/\W+/g, "-")}.png` })
        .catch(() => {});
      await page.goto(`https://${real.host}/`).catch(() => {});
      const r = { label, ok: false, ms: Date.now() - t, shown, error: String(e).slice(0, 200) };
      asks.push(r);
      record(LOG, { ask: r });
      return r;
    }
  };
  await timedAsk("paired");
  const idleStart = Date.now();
  let minute = 0;
  while (Date.now() - idleStart < IDLE_MINUTES * 60_000) {
    await sleep(30_000);
    keep("phone idle");
    const m = Math.floor((Date.now() - idleStart) / 60_000);
    if (m >= minute + 10) {
      minute = m;
      await timedAsk(`idle ${m} min`);
      const s = await state();
      record(LOG, {
        step: "state",
        minute: m,
        type: s.type,
        sameAddress: s.address?.includes(real.host),
      });
    }
  }
  const events = await netEvents(page);
  record(LOG, {
    step: "idle done",
    minutes: IDLE_MINUTES,
    socketOpens: events.filter((e) => e.kind === "ws-open").length,
    socketCloses: events
      .filter((e) => e.kind === "ws-close")
      .map((e) => new Date(e.t).toISOString()),
  });

  // A 50 MB upload: the app's peak while it streams to disk.
  const before = sample("before upload");
  let peakApp = before.appKb as number;
  let peakHelper = (before.helperKb as number | null) ?? 0;
  const t = Date.now();
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({
      name: "large.jpg",
      mimeType: "image/jpeg",
      buffer: photo("LARGE-UPLOAD", 49_500_000),
    });
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  const done = page.getByRole("heading", { name: "Kept." });
  let kept = false;
  while (Date.now() - t < 600_000) {
    const s = sample("upload");
    peakApp = Math.max(peakApp, s.appKb as number);
    peakHelper = Math.max(peakHelper, (s.helperKb as number | null) ?? 0);
    if (await done.isVisible()) {
      kept = true;
      break;
    }
    await sleep(200);
  }
  record(LOG, {
    check: 9,
    phase: "50 MB upload",
    kept,
    seconds: Math.round((Date.now() - t) / 1000),
    appBeforeKb: before.appKb,
    appPeakKb: peakApp,
    appGrowthKb: peakApp - (before.appKb as number),
    helperPeakKb: peakHelper,
    partsLeft: partials(real.data).length,
    asks,
  });
  console.log(
    `upload kept=${kept} app growth ${peakApp - (before.appKb as number)} kB, helper peak ${peakHelper} kB`,
  );
  if (kept) await home(page);

  // The helper freezes for a minute (the edge loses it), then carries on.
  const frozen = helperPids(real.data)[0]?.pid ?? 0;
  process.kill(frozen, "SIGSTOP");
  const stopAt = Date.now();
  const during: string[] = [];
  while (Date.now() - stopAt < 60_000) {
    during.push((await state()).type);
    await sleep(5000);
  }
  process.kill(frozen, "SIGCONT");
  const contAt = Date.now();
  let backMs = -1;
  while (Date.now() - contAt < 420_000) {
    const r = await fetch(`https://${real.host}/manifest.webmanifest`, {
      signal: AbortSignal.timeout(10_000),
    }).then(
      (x) => x.status,
      () => 0,
    );
    if (r === 200) {
      backMs = Date.now() - contAt;
      break;
    }
    await sleep(1000);
  }
  const afterFreeze = await timedAsk("after a one-minute freeze");
  const s1 = await state();
  record(LOG, {
    step: "freeze",
    statesDuring: [...new Set(during)],
    servingAgainAfterMs: backMs,
    sameAddress: s1.address?.includes(real.host),
    restarts: helperPids(real.data)[0]?.pid !== frozen,
    ask: afterFreeze,
  });

  // The helper is killed: the supervisor starts it again under a new address.
  const old = real.host;
  const killed = helperPids(real.data)[0]?.pid ?? 0;
  process.kill(killed, "SIGKILL");
  const killAt = Date.now();
  const seen: string[] = [];
  let relink: { type: string; address?: string; pairingLink?: string } | null = null;
  while (Date.now() - killAt < 300_000) {
    const s = await state();
    if (seen.at(-1) !== s.type) seen.push(s.type);
    if (s.type === "needsRelink" || (s.type === "on" && s.address && !s.address.includes(old))) {
      relink = s;
      break;
    }
    await sleep(1000);
  }
  const newHost = relink?.address?.replace(/^https:\/\//, "").replace(/\/$/, "") ?? "";
  const nameAgain = newHost ? await waitForName(newHost) : null;
  const oldServes: Array<Record<string, unknown>> = [];
  for (const wait of [0, 60, 240]) {
    await sleep(wait * 1000);
    const r = await fetch(`https://${old}/p`, { signal: AbortSignal.timeout(20_000) }).then(
      async (x) => ({
        status: x.status,
        server: x.headers.get("server"),
        body: (await x.text()).replace(/\s+/g, " ").slice(0, 120),
      }),
      (e) => ({ error: String((e as Error).cause ?? e).slice(0, 120) }),
    );
    oldServes.push({ afterSecs: Math.round((Date.now() - killAt) / 1000), ...r });
  }
  record(LOG, {
    step: "helper killed",
    states: seen,
    newAddressAfterMs: relink ? Date.now() - killAt : -1,
    newHost,
    nameResolvedAfter: nameAgain,
    oldServes,
  });
  // The phone stays on its home address (its app comes from its own cache now) and re-links by
  // the four words of the new address, as a person would type them.
  let relinked: Record<string, unknown> = { tried: false };
  if (newHost) {
    real.host = newHost;
    const t = Date.now();
    // Home on the old address: the phone app comes from its own cache there.
    await page.goto(`https://${old}/`).catch(() => {});
    // The app may come back on the screen it was on; the new-address card is on Home.
    await page
      .getByRole("button", { name: /^(Back to home|Home)$/ })
      .first()
      .click({ timeout: 10_000 })
      .catch(() => {});
    const asleep = await page
      .getByText("Has your computer got a new address?")
      .waitFor({ timeout: 120_000 })
      .then(
        () => true,
        () => false,
      );
    let ok = false;
    if (asleep) {
      await page.getByRole("button", { name: "Type the words instead" }).click();
      const words = newHost.replace(".trycloudflare.com", "").split("-");
      for (const [i, w] of words.entries()) await page.getByLabel(`Word ${i + 1}`).fill(w);
      await page.getByRole("button", { name: "Reconnect" }).click();
      ok = await page
        .getByText(/^Connected to /)
        .waitFor({ timeout: 60_000 })
        .then(
          () => true,
          () => false,
        );
    }
    const shown = ok
      ? ""
      : await page.evaluate(() => document.body.textContent?.slice(0, 400) ?? "");
    relinked = {
      tried: true,
      sawAsleep: asleep,
      words: newHost.split(".")[0]?.split("-").length,
      ok,
      ms: Date.now() - t,
      shown,
    };
    if (ok) relinked.ask = await timedAsk("after re-linking");
  }
  record(LOG, { step: "relink", relinked, stateNow: (await state()).type });

  await context.close();
  expect(kept).toBe(true);
  expect(relinked.ok).toBe(true);
});
