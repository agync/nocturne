// Checks the served site: axe on the landing page, the install page, the support page, their German and French
// copies and a few docs pages in both themes, no policy violations or console errors, no sideways
// scrolling at 360 px (320 px for the install page), the theme buttons work from the keyboard, and
// full-page screenshots at 1440 and 390 px.
//
//   make serve                                  # in another terminal
//   node site/scripts/check.mjs http://127.0.0.1:4180
//
// SHOTS_DIR chooses where the screenshots go (default site/build/check). UI_DIR and CHROME_PATH
// work as in shots.mjs.

import { mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const ui = process.env.UI_DIR ?? path.join(root, "ui");
const { chromium } = await import(path.join(ui, "node_modules/playwright/index.mjs"));
const { default: AxeBuilder } = await import(
  path.join(ui, "node_modules/@axe-core/playwright/dist/index.mjs")
);

const origin = process.argv[2] ?? "http://127.0.0.1:4180";
const shots = process.env.SHOTS_DIR ?? path.join(root, "site/build/check");
mkdirSync(shots, { recursive: true });

const pages = [
  { name: "landing", path: "/", shoot: true },
  { name: "install", path: "/install", shoot: true },
  { name: "support", path: "/support", shoot: true },
  { name: "docs-home", path: "/docs/", shoot: true },
  { name: "docs-concept", path: "/docs/design/concept.html", shoot: false },
  { name: "docs-install", path: "/docs/guide/install.html", shoot: false },
  { name: "not-found", path: "/no-such-page", shoot: false },
  { name: "landing-de", path: "/de/", shoot: true },
  { name: "install-de", path: "/de/install", shoot: true },
  { name: "support-de", path: "/de/support", shoot: true },
  { name: "landing-fr", path: "/fr/", shoot: false },
  { name: "not-found-nl", path: "/nl/no-such-page", shoot: false },
];
const sizes = [
  { name: "1440", width: 1440, height: 900 },
  { name: "390", width: 390, height: 844 },
];

// Findings in markup the okf-wiki renderer writes itself (the sidebar's linked section titles,
// wide tables, the search button and the diagram zoom control). The theme cannot change them, so
// they are reported, not failed; they belong to the renderer.
const RENDERER = new Set(["nested-interactive", "scrollable-region-focusable", "label-content-name-mismatch"]);
const warnings = [];
const warn = (msg) => {
  warnings.push(msg);
  console.log(`WARN ${msg}`);
};

const failures = [];
const fail = (msg) => {
  failures.push(msg);
  console.log(`FAIL ${msg}`);
};

const browser = await chromium.launch(
  process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {},
);
try {
  for (const theme of ["light", "dark"]) {
    for (const size of sizes) {
      const context = await browser.newContext({
        viewport: { width: size.width, height: size.height },
        deviceScaleFactor: size.width < 600 ? 2 : 1,
        colorScheme: theme,
        reducedMotion: "reduce",
      });
      for (const p of pages) {
        const page = await context.newPage();
        const problems = [];
        page.on("console", (m) => {
          if (m.type() === "error") problems.push(`console: ${m.text().slice(0, 200)}`);
        });
        page.on("pageerror", (e) => problems.push(`pageerror: ${e.message.slice(0, 200)}`));
        const response = await page.goto(origin + p.path, { waitUntil: "networkidle" });
        const expected = p.name.startsWith("not-found") ? 404 : 200;
        if (response.status() !== expected) fail(`${p.name}: status ${response.status()}`);
        await page.evaluate(async () => {
          for (const img of document.querySelectorAll("img[loading=lazy]")) img.loading = "eager";
          await document.fonts.ready;
        });
        await page.waitForLoadState("networkidle");
        await page.waitForTimeout(400);

        const axe = await new AxeBuilder({ page })
          .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
          .analyze();
        for (const v of axe.violations) {
          const line = `${p.name} ${theme} ${size.name}: axe ${v.id} (${v.nodes.length}) ${v.help}`;
          if (p.path.startsWith("/docs/") && RENDERER.has(v.id)) warn(line);
          else fail(line);
        }
        const wide = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        if (wide > 0) fail(`${p.name} ${theme} ${size.name}: scrolls sideways by ${wide}px`);
        for (const problem of problems.filter((x) => !x.includes("404 (Not Found)"))) {
          fail(`${p.name} ${theme} ${size.name}: ${problem}`);
        }
        if (p.shoot) {
          const file = path.join(shots, `${p.name}-${size.name}-${theme}.png`);
          await page.screenshot({ path: file, fullPage: true });
          console.log(`shot ${file}`);
        }
        console.log(`ok ${p.name} ${theme} ${size.name}: axe ${axe.violations.length}, passes ${axe.passes.length}`);
        await page.close();
      }
      await context.close();
    }
  }

  // Narrowest supported width, and the theme buttons from the keyboard.
  const context = await browser.newContext({ viewport: { width: 360, height: 740 } });
  const page = await context.newPage();
  await page.goto(`${origin}/`, { waitUntil: "networkidle" });
  const wide = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (wide > 0) fail(`landing 360: scrolls sideways by ${wide}px`);
  else console.log("ok landing 360: no sideways scroll");
  await page.setViewportSize({ width: 320, height: 740 });
  await page.goto(`${origin}/install`, { waitUntil: "networkidle" });
  const installWide = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (installWide > 0) fail(`install 320: scrolls sideways by ${installWide}px`);
  else console.log("ok install 320: no sideways scroll");
  await page.setViewportSize({ width: 360, height: 740 });
  await page.goto(`${origin}/`, { waitUntil: "networkidle" });
  await page.keyboard.press("Tab");
  const skip = await page.evaluate(() => document.activeElement?.textContent?.trim());
  if (skip !== "Skip to content") fail(`first Tab lands on ${JSON.stringify(skip)}`);
  let reached = false;
  for (let i = 0; i < 12 && !reached; i++) {
    await page.keyboard.press("Tab");
    reached = await page.evaluate(() => document.activeElement?.dataset?.themeChoice === "dark");
  }
  if (!reached) fail("the Dark button is not reachable with Tab");
  await page.keyboard.press("Enter");
  const state = await page.evaluate(() => ({
    theme: document.documentElement.dataset.theme,
    pressed: document.querySelector("[data-theme-choice=dark]").getAttribute("aria-pressed"),
  }));
  if (state.theme !== "dark" || state.pressed !== "true") fail(`Dark button: ${JSON.stringify(state)}`);
  await page.goto(`${origin}/docs/`, { waitUntil: "networkidle" });
  const docsTheme = await page.evaluate(() => document.documentElement.dataset.theme);
  if (docsTheme !== "dark") fail(`the docs did not keep the dark choice (${docsTheme})`);
  else console.log("ok keyboard: theme buttons work and the choice carries into the docs");
  await context.close();
} finally {
  await browser.close();
}

console.log(`warnings (renderer): ${warnings.length}`);
console.log(failures.length ? `FAILED: ${failures.length}` : "PASSED");
process.exit(failures.length ? 1 : 0);
