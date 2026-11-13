import { existsSync } from "node:fs";
import { chromium, defineConfig } from "@playwright/test";

// The owner-approved real-network trials (plan 12, D10): the phone app in Chromium against the
// computer through the real cloudflared or tailnet helper. Never run by `make check` or CI; run
// through scripts/away-trials.sh, which says what it reaches.
function chromiumChannel(): string | undefined {
  if (process.env.PW_CHANNEL) return process.env.PW_CHANNEL;
  return existsSync(chromium.executablePath()) ? undefined : "chrome";
}

export default defineConfig({
  testDir: "./e2e-trials",
  outputDir: "./test-results/trials",
  fullyParallel: false,
  workers: 1,
  timeout: 600_000,
  expect: { timeout: 30_000 },
  reporter: [["list"]],
  use: {
    channel: chromiumChannel(),
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    deviceScaleFactor: 2,
    isMobile: true,
    locale: "en-GB",
    timezoneId: "Europe/London",
    screenshot: "only-on-failure",
  },
});
