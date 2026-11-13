import { test } from "@playwright/test";
import { expect } from "../e2e-away/fixtures";
import { instrument, netEvents } from "../e2e-away/phone";
import { ask, home, pairScanned, record, sleep, startReal, waitForName } from "./trial";

test("quiet socket: first ask after three quiet minutes", async ({ browser }) => {
  const real = await startReal("simple");
  real.host = String(await real.driver.call("wait_on", { secs: 300 }));
  await waitForName(real.host);
  const context = await browser.newContext();
  const page = await context.newPage();
  await instrument(page, "ws");
  await pairScanned(page, real, "Quiet trial");
  const t0 = Date.now();
  const asks: Array<Record<string, number>> = [];
  const quiets = (process.env.TRIAL_QUIET ?? "30,90,150").split(",").map(Number);
  test.setTimeout((quiets.reduce((a, b) => a + b, 0) + 600) * 1000);
  for (const quiet of quiets) {
    await sleep(quiet * 1000);
    const t = Date.now();
    await ask(page, "When is the boiler serviced?");
    await expect(page.getByText("The boiler is serviced every spring.").first()).toBeVisible({
      timeout: 60_000,
    });
    asks.push({ quietSecs: quiet, ms: Date.now() - t, at: t - t0 });
    await home(page);
  }
  const events = (await netEvents(page)).map((e) => ({ kind: e.kind, at: e.t - t0 }));
  record("simple-quiet.jsonl", { asks, events });
  console.log(JSON.stringify({ asks, events }));
  await context.close();
  await real.driver.stop();
});
