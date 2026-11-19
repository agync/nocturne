// Captures the app screenshots the landing page shows, from the mock interface and its demo data,
// and writes them as WebP at the widths the page asks for.
//
//   cd ui && npx vite --mode mock --port 4173     # in another terminal
//   node site/scripts/shots.mjs http://127.0.0.1:4173 [phone origin]
//
// SHOTS names the shots to take, separated by commas (SHOTS=journal,look); without it, every
// shot is taken.
//
// The phone shot needs the phone app's scripted build, served on its own. Without it the phone
// shot is skipped:
//
//   cd ui && VITE_ENGINE=phone-mock PHONE_OUT_DIR=/tmp/phone npx vite build --config vite.phone.config.ts
//   python3 -m http.server -d /tmp/phone --bind 127.0.0.1 4174
//
// The desktop shots are the window's column right of the rail, captured at a width where the
// kept-today column has moved under the page, so every one has the same frame and only the page
// in it changes. The morning page step has no narrow shot: on narrow screens it would repeat the
// picture at the top of the page, so the page hides it. A shot with `mark` prints where that
// element sits, in percent of the picture; the page's tour uses those numbers to show where to
// look.
//
// The journal shots set the mock clock to Thursday 12 November in the evening, the day the hero's
// Friday page looks back on. The journal window is cut on a blank line, below the check-ins on wide
// screens and below the entry's words on narrow ones, so the card of the day looked back on, which
// the page lays over the window's foot, covers no words. That card is the reflection's first
// section alone, on the sheet colour. The phone shot is tall enough to show the note under the
// tiles that things marked private stay on the computer.
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
const phoneOrigin = process.argv[3] ?? "";
const out = path.join(root, "site/assets/img");
const work = mkdtempSync(path.join(tmpdir(), "nocturne-shots-"));

const desktop = { width: 1280, height: 800 };
const column = { width: 1000, height: 900 };
const phone = { width: 390, height: 760 };
const ASK = "What does Leo need for his trip?";
const THURSDAY = "2026-11-12T21:40:00Z";

// Each shot is captured in both themes. `widths` are the WebP widths written, in pixels. `height`
// cuts a shot that many pixels from its top; `pad` frames a clipped element.
const shots = [
  { name: "page", route: "/#/today", viewport: desktop, clip: "main article", height: 625, widths: [560, 1120] },
  { name: "page-narrow", route: "/#/today", viewport: phone, widths: [390, 780] },
  { name: "keep", route: "/#/quick-capture", viewport: { width: 640, height: 220 }, box: true, widths: [560, 1120] },
  { name: "recent", route: "/#/recent", viewport: column, clip: "column", mark: "li[class*=_row_]", widths: [768, 1536] },
  { name: "recent-narrow", route: "/#/recent", viewport: phone, clip: "main", widths: [390, 780] },
  { name: "ask", route: "/#/ask", viewport: column, clip: "column", ask: ASK, mark: "[class*=_receipts_]", widths: [768, 1536] },
  { name: "ask-narrow", route: "/#/ask", viewport: phone, clip: "main", ask: ASK, widths: [390, 780] },
  { name: "today", route: "/#/today", viewport: column, clip: "column", mark: "[class*=_line_]:has(fieldset)", widths: [768, 1536] },
  { name: "journal", route: "/#/journal", now: THURSDAY, viewport: column, clip: "column", height: 702, widths: [768, 1536] },
  { name: "journal-narrow", route: "/#/journal", now: THURSDAY, viewport: phone, clip: "main", height: 475, widths: [390, 780] },
  { name: "look", route: "/#/journal/look-back/day/2026-11-12", now: THURSDAY, viewport: { width: 880, height: 900 }, clip: "main article section", pad: 16, widths: [560, 1120] },
  { name: "phone", phone: true, route: "/", viewport: { width: 390, height: 704 }, widths: [390, 780] },
];

async function settle(page) {
  await page.waitForLoadState("networkidle").catch(() => {});
  await page
    .waitForFunction(() => !/Loading/.test(document.body?.innerText ?? ""), null, { timeout: 15000 })
    .catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
}

const only = process.env.SHOTS ? new Set(process.env.SHOTS.split(",")) : null;

const browser = await chromium.launch(
  process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {},
);
try {
  for (const theme of ["light", "dark"]) {
    for (const shot of shots) {
      if (only && !only.has(shot.name)) continue;
      if (shot.phone && !phoneOrigin) {
        console.log(`skipped ${shot.name}: no phone origin`);
        continue;
      }
      const context = await browser.newContext({
        viewport: shot.viewport,
        deviceScaleFactor: 2,
        colorScheme: theme,
        reducedMotion: "reduce",
        ...(shot.now ? { timezoneId: "Europe/London" } : {}),
      });
      const page = await context.newPage();
      const route = shot.now ? shot.route.replace("/#", `/?mockNow=${shot.now}#`) : shot.route;
      await page.goto((shot.phone ? phoneOrigin : origin) + route);
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
      let area = null;
      if (shot.clip === "column" || shot.clip === "main") {
        const r = await page.locator("main").first().boundingBox();
        area =
          shot.clip === "column"
            ? { x: r.x, y: 0, width: shot.viewport.width - r.x, height: shot.viewport.height }
            : r;
        if (shot.height) area.height = shot.height;
      }
      if (shot.mark && area && theme === "light") {
        const m = await page.locator(shot.mark).first().boundingBox();
        const pct = (v, of) => ((v / of) * 100).toFixed(1);
        console.log(
          `${shot.name} mark: ${pct(m.x - area.x, area.width)} ${pct(m.y - area.y, area.height)} ` +
            `${pct(m.width, area.width)} ${pct(m.height, area.height)}`,
        );
      }
      const png = path.join(work, `${shot.name}-${theme}.png`);
      if (area) {
        await page.screenshot({ path: png, clip: area });
      } else if (shot.pad) {
        // The section's hairline sits on its top edge; its own padding stands in for the top pad.
        const r = await page.locator(shot.clip).first().boundingBox();
        await page.screenshot({
          path: png,
          clip: { x: r.x - shot.pad, y: r.y + 1, width: r.width + shot.pad * 2, height: r.height - 1 + shot.pad },
        });
      } else if (shot.clip) {
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
