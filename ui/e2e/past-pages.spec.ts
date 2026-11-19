import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page) => {
  await page.goto("/#/pages");
  await page.getByRole("heading", { level: 1, name: "Past pages" }).waitFor();
  await page.getByRole("heading", { level: 2, name: "November 2026" }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("screenshots of #/pages", () => {
  test("light", async ({ page }) => {
    await ready(page);
    await page.screenshot({ path: `${SHOTS}/past-pages-light.png` });
  });

  test("dark", async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: "dark",
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await ready(page);
    await page.screenshot({ path: `${SHOTS}/past-pages-dark.png` });
    await context.close();
  });

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("400 wide", async ({ page }) => {
      await ready(page);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      await page.screenshot({ path: `${SHOTS}/past-pages-narrow.png` });
    });
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on past pages in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    await ready(page);
    const results = await runAxe(page, (b) => b.withTags(TAGS));
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await context.close();
  });
}

test.describe("using it", () => {
  test("every page is a link that the keyboard can reach and open", async ({ page }) => {
    await ready(page);
    const first = page.getByRole("link", { name: /Three things need you today/ });
    await first.focus();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: /One thing needs you today/ })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/pages\/2026-11-12$/);
    await expect(
      page.getByRole("heading", { level: 1, name: "One thing needs you today." }),
    ).toBeVisible();
  });

  test("the rail keeps Past pages marked while the list is open", async ({ page }) => {
    await ready(page);
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Past pages" }),
    ).toHaveAttribute("aria-current", "page");
  });
});
