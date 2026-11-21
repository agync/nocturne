import { existsSync } from "node:fs";
import { chromium, defineConfig } from "@playwright/test";

// The phone app's screens in a browser, at an Android phone's size: the native build's screens
// with the scripted phone engine, and the app's native side answered in the page (calendar,
// Health, This phone, shares). No phone, no computer, no network.
const PORT = Number(process.env.PW_PORT ?? 4174) + 200;
const OUT = "test-results/phone-native-build";

function chromiumChannel(): string | undefined {
  if (process.env.PW_CHANNEL) return process.env.PW_CHANNEL;
  return existsSync(chromium.executablePath()) ? undefined : "chrome";
}

export default defineConfig({
  testDir: "./e2e-phone-native",
  outputDir: "./test-results/phone-native",
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    baseURL: `http://localhost:${PORT}`,
    channel: chromiumChannel(),
    viewport: { width: 412, height: 892 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
    locale: "en-GB",
    timezoneId: "Europe/London",
  },
  webServer: {
    command: `VITE_NATIVE_MOCK=1 npx vite build --config vite.phone.config.ts --mode test --outDir ../${OUT} --emptyOutDir && npx vite preview --config vite.phone.config.ts --outDir ../${OUT} --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: process.env.PW_REUSE === "1",
    timeout: 120_000,
  },
});
