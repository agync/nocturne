// Captures the Open Graph image (1200 x 630) from the top of the served landing page.
//
//   make serve                                  # in another terminal
//   node site/scripts/og.mjs http://127.0.0.1:4180
//
// UI_DIR and CHROME_PATH work as in shots.mjs.

import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const ui = process.env.UI_DIR ?? path.join(root, "ui");
const { chromium } = await import(path.join(ui, "node_modules/playwright/index.mjs"));

const origin = process.argv[2] ?? "http://127.0.0.1:4180";
const browser = await chromium.launch(
  process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {},
);
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, colorScheme: "light" });
  await page.goto(`${origin}/`, { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: path.join(root, "site/assets/img/og.png") });
} finally {
  await browser.close();
}
