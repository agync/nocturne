// Captures the app screenshots the landing page shows, from the mock interface and its demo data,
// and writes them as WebP at the widths the page asks for.
//
//   cd ui && npx vite --mode mock --port 4173     # in another terminal
//   node site/scripts/shots.mjs http://127.0.0.1:4173
//
// Needs the UI dependencies (Playwright) and ImageMagick (`magick`) with WebP. UI_DIR names another
// checkout's ui/ when this one has no node_modules. CHROME_PATH points at a Chromium build when
// Playwright's own is not installed.

import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const ui = process.env.UI_DIR ?? path.join(root, "ui");
const { chromium } = await import(path.join(ui, "node_modules/playwright/index.mjs"));

const origin = process.argv[2] ?? "http://127.0.0.1:4173";
const out = path.join(root, "site/assets/img");
const work = mkdtempSync(path.join(tmpdir(), "nocturne-shots-"));

const desktop = { width: 1280, height: 800 };
const phone = { width: 390, height: 760 };

// Each shot is captured in both themes. `widths` are the WebP widths written, in pixels.
const shots = [
  { name: "page", route: "/#/today", viewport: desktop, clip: "main article", height: 625, widths: [560, 1120] },
  { name: "page-narrow", route: "/#/today", viewport: phone, widths: [390, 780] },
  { name: "keep", route: "/#/quick-capture", viewport: { width: 640, height: 220 }, box: true, widths: [560, 1120] },
  { name: "recent", route: "/#/recent", viewport: desktop, widths: [800, 1600] },
  { name: "recent-narrow", route: "/#/recent", viewport: phone, widths: [390, 780] },
  { name: "ask", route: "/#/ask", viewport: desktop, ask: "What does Leo need for his trip?", widths: [800, 1600] },
  { name: "ask-narrow", route: "/#/ask", viewport: phone, ask: "What does Leo need for his trip?", widths: [390, 780] },
  { name: "fading", route: "/#/fading", viewport: desktop, widths: [800, 1600] },
  { name: "fading-narrow", route: "/#/fading", viewport: phone, widths: [390, 780] },
  { name: "away", route: "/?mockAway=on#/settings/away", viewport: desktop, widths: [800, 1600] },
  { name: "away-narrow", route: "/?mockAway=on#/settings/away", viewport: phone, widths: [390, 780] },
];

async function settle(page) {
  await page.waitForLoadState("networkidle").catch(() => {});
  await page
    .waitForFunction(() => !/Loading/.test(document.body?.innerText ?? ""), null, { timeout: 15000 })
    .catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
}

const browser = await chromium.launch(
  process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {},
);
try {
  for (const theme of ["light", "dark"]) {
    for (const shot of shots) {
      const context = await browser.newContext({
        viewport: shot.viewport,
        deviceScaleFactor: 2,
        colorScheme: theme,
        reducedMotion: "reduce",
      });
      const page = await context.newPage();
      await page.goto(origin + shot.route);
      await settle(page);
      if (shot.ask) {
        const box = page.getByRole("textbox").first();
        await box.click();
        await box.pressSequentially(shot.ask, { delay: 15 });
        await page.keyboard.press("Enter");
        await page.waitForTimeout(4000);
        await page.evaluate(() => document.activeElement?.blur());
        await page.mouse.move(0, 0);
        await settle(page);
      }
      if (shot.box) {
        const box = page.getByRole("textbox").first();
        await box.click();
        await box.pressSequentially("Gran’s new number ends 4471", { delay: 15 });
      }
      const png = path.join(work, `${shot.name}-${theme}.png`);
      if (shot.clip) {
        const r = await page.locator(shot.clip).first().boundingBox();
        await page.screenshot({
          path: png,
          clip: { x: r.x, y: r.y, width: r.width, height: Math.min(shot.height, r.height) },
        });
      } else if (shot.box) {
        const r = await page.locator("[class*=_surface_]").first().boundingBox();
        const pad = 24;
        await page.screenshot({
          path: png,
          clip: { x: r.x - pad, y: r.y - pad, width: r.width + pad * 2, height: r.height + pad * 2 },
        });
      } else {
        await page.screenshot({ path: png });
      }
      for (const w of shot.widths) {
        const file = path.join(out, `${shot.name}-${theme}-${w}.webp`);
        execFileSync("magick", [png, "-resize", `${w}x`, "-strip", "-quality", "82", file]);
        console.log(path.relative(root, file));
      }
      await context.close();
    }
  }
} finally {
  await browser.close();
  rmSync(work, { recursive: true, force: true });
}
