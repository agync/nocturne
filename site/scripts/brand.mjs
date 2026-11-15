// Renders the brand images from their sources in site/brand/ and the favicon:
//
//   node site/scripts/brand.mjs
//
//   site/brand/card.html      -> docs/assets/social-preview.png (1280 x 640, the GitHub preview)
//                             -> site/assets/img/og.png (1200 x 630, the Open Graph image)
//   docs/assets/logo.svg      -> docs/assets/logo.png (512 x 512)
//   site/brand/touch-icon.svg -> site/apple-touch-icon.png (180 x 180, square; the phone rounds it)
//   site/favicon.svg          -> site/favicon-32.png (32 x 32)
//
// UI_DIR and CHROME_PATH work as in shots.mjs.

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const ui = process.env.UI_DIR ?? path.join(root, "ui");
const { chromium } = await import(path.join(ui, "node_modules/playwright/index.mjs"));

const at = (p) => path.join(root, p);

const cards = [
  { out: "docs/assets/social-preview.png", width: 1280, height: 640 },
  { out: "site/assets/img/og.png", width: 1200, height: 630 },
];

const icons = [
  { src: "docs/assets/logo.svg", out: "docs/assets/logo.png", size: 512 },
  { src: "site/brand/touch-icon.svg", out: "site/apple-touch-icon.png", size: 180 },
  { src: "site/favicon.svg", out: "site/favicon-32.png", size: 32 },
];

const browser = await chromium.launch(
  process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {},
);
try {
  for (const card of cards) {
    const page = await browser.newPage({ viewport: { width: card.width, height: card.height } });
    await page.goto(pathToFileURL(at("site/brand/card.html")).href, { waitUntil: "load" });
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: at(card.out) });
    await page.close();
    console.log(`brand.mjs: ${card.out}`);
  }
  for (const icon of icons) {
    const page = await browser.newPage({ viewport: { width: icon.size, height: icon.size } });
    const svg = readFileSync(at(icon.src), "utf8").replace("<svg ", `<svg width="${icon.size}" height="${icon.size}" `);
    await page.setContent(`<style>html,body{margin:0;background:transparent}svg{display:block}</style>${svg}`);
    await page.screenshot({ path: at(icon.out), omitBackground: true });
    await page.close();
    console.log(`brand.mjs: ${icon.out}`);
  }
} finally {
  await browser.close();
}
