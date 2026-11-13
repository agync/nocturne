import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const HEAVY = "2026-01-01";

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

/** How many pages a PDF has, from its page tree. */
const pagesIn = (pdf: Buffer): number => {
  const counts = [...pdf.toString("latin1").matchAll(/\/Type\s*\/Pages[^>]*?\/Count\s+(\d+)/g)];
  const total = counts.map((m) => Number(m[1]));
  return total.length > 0
    ? Math.max(...total)
    : (pdf.toString("latin1").match(/\/Type\s*\/Page\b(?!s)/g) ?? []).length;
};

test.describe("Listen", () => {
  const ready = async (page: Page) => {
    await page.goto("/#/listen/2026-11-13");
    await page.getByRole("list", { name: "Sections" }).waitFor();
    await page.evaluate(() => document.fonts.ready);
  };

  test("plays the spoken page and keeps the keys", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.getByRole("button", { name: "Play" }).click();
    await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
    const time = () => page.locator("audio").evaluate((a: HTMLAudioElement) => a.currentTime);
    await expect.poll(time, { timeout: 5000 }).toBeGreaterThan(0.2);
    await page.getByRole("button", { name: "Pause" }).click();
    await page.getByRole("button", { name: "Next section" }).click();
    await expect(page.getByRole("listitem").nth(1)).toHaveAttribute("aria-current", "true");
    await page.keyboard.press("]");
    await expect(page.getByLabel("Speed")).toHaveText("1.1×");
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`has no accessibility violations in ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await ready(page);
      expect(await axe(page)).toEqual([]);
    });
  }
});

test.describe("Print", () => {
  const ready = async (page: Page, date = HEAVY) => {
    await page.goto(`/#/print/${date}`);
    await page.getByRole("article", { name: "Print" }).waitFor();
    await page.evaluate(() => document.fonts.ready);
  };

  for (const [size, label] of [
    ["a4", "A4"],
    ["letter", "Letter"],
    ["receipt", /Receipt/],
  ] as const) {
    test(`the heaviest page is one sheet on ${size}`, async ({ page, browserName }) => {
      test.skip(browserName !== "chromium", "page.pdf is Chromium only");
      await ready(page);
      await page.getByRole("radio", { name: label }).check();
      const pdf = await page.pdf({ preferCSSPageSize: true, printBackground: true });
      expect(pagesIn(pdf)).toBe(1);
    });
  }

  test("an ordinary page is one sheet too, and the toolbar does not print", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "page.pdf is Chromium only");
    await ready(page, "2026-11-13");
    const pdf = await page.pdf({ preferCSSPageSize: true });
    expect(pagesIn(pdf)).toBe(1);
    await page.emulateMedia({ media: "print" });
    await expect(page.getByRole("toolbar")).toBeHidden();
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`has no accessibility violations in ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await ready(page);
      expect(await axe(page)).toEqual([]);
    });
  }
});
