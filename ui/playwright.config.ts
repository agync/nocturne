import { existsSync } from "node:fs";
import { chromium, defineConfig, devices, webkit } from "@playwright/test";

// Each worktree can run its own preview: set PW_PORT to avoid serving another lane's build.
const PORT = Number(process.env.PW_PORT ?? 4174);

function chromiumChannel(): string | undefined {
  if (process.env.PW_CHANNEL) return process.env.PW_CHANNEL;
  return existsSync(chromium.executablePath()) ? undefined : "chrome";
}

const projects = [
  {
    name: "chromium",
    use: { ...devices["Desktop Chrome"], channel: chromiumChannel() },
  },
];

if (existsSync(webkit.executablePath())) {
  projects.push({
    name: "webkit",
    use: { ...devices["Desktop Safari"], channel: undefined },
  });
}

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results/artifacts",
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    baseURL: `http://localhost:${PORT}`,
    locale: "en-GB",
    timezoneId: "Europe/London",
    viewport: { width: 1280, height: 800 },
  },
  projects,
  webServer: {
    command: `npx vite build --mode mock && npx vite preview --mode mock --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    // Reusing a running server is opt-in, so a stale or foreign build is never tested by accident.
    reuseExistingServer: process.env.PW_REUSE === "1",
    timeout: 120_000,
  },
});
