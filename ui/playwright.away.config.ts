import { existsSync } from "node:fs";
import { chromium, defineConfig } from "@playwright/test";

// The phone app against the real computer through the stand-in helper. There is no web server:
// the computer is the Rust program `e2e-away/world.ts` starts, and the phone's page comes from it.
// Run through scripts/away-integration.sh, which builds both sides first.
function chromiumChannel(): string | undefined {
  if (process.env.PW_CHANNEL) return process.env.PW_CHANNEL;
  return existsSync(chromium.executablePath()) ? undefined : "chrome";
}

export default defineConfig({
  testDir: "./e2e-away",
  outputDir: "./test-results/away",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  reporter: [["list"]],
  use: {
    channel: chromiumChannel(),
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    deviceScaleFactor: 2,
    isMobile: true,
    locale: "en-GB",
    timezoneId: "Europe/London",
    ignoreHTTPSErrors: true,
    screenshot: "only-on-failure",
  },
});
