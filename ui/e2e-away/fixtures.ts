import { test as base, chromium } from "@playwright/test";
import { type Computer, Front, startComputer } from "./world";

export interface NetEvent {
  t: number;
  kind: "poll-start" | "poll-end" | "ws-open" | "ws-close";
}

// Chromium reaches the computer's address through the front. The browser is started once per
// worker, with that mapping; each test gets its own computer, so no pairing outlives a test.
export const test = base.extend<{ computer: Computer }, { front: Front }>({
  front: [
    // biome-ignore lint/correctness/noEmptyPattern: Playwright reads the fixtures from the pattern
    async ({}, use) => {
      const front = new Front();
      await front.start();
      await use(front);
      await front.close();
    },
    { scope: "worker" },
  ],
  browser: [
    async ({ front }, use, workerInfo) => {
      const channel = workerInfo.project.use.channel;
      const browser = await chromium.launch({ channel, args: front.args });
      await use(browser);
      await browser.close();
    },
    { scope: "worker" },
  ],
  computer: async ({ front }, use) => {
    const computer = await startComputer(front);
    await use(computer);
    await computer.driver.stop();
  },
});

export { expect } from "@playwright/test";
