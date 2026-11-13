import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const HEAVY = "2026-01-01";
const SHOTS = "test-results/screens";

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

/** The height of the first page of a PDF, in millimetres, from its media box. */
const pageHeightMm = (pdf: Buffer): number => {
  const box = /\/MediaBox\s*\[\s*[\d.]+\s+[\d.]+\s+([\d.]+)\s+([\d.]+)\s*\]/.exec(
    pdf.toString("latin1"),
  );
  return (Number(box?.[2]) * 25.4) / 72;
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
    await expect(
      page.getByRole("list", { name: "Sections" }).getByRole("listitem").nth(1).getByRole("button"),
    ).toHaveAttribute("aria-current", "true");
    await page.keyboard.press("]");
    await expect(page.getByTestId("speed")).toContainText("1.1×");
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`has no accessibility violations in ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await ready(page);
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/listen-${scheme}.png` });
    });
  }

  test("fits a narrow window", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await ready(page);
    expect(await axe(page)).toEqual([]);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/listen-narrow.png` });
  });
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
      if (size !== "a4") return;
      await page.screenshot({ path: `${SHOTS}/print-a4.png`, fullPage: true });
    });
  }

  test("a receipt is as long as its sheet, and no text on it is smaller than 9 pt", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "page.pdf is Chromium only");
    await ready(page);
    await page.getByRole("radio", { name: /Receipt/ }).check();
    const sheet = page.getByRole("article", { name: "Print" });
    await expect(sheet).toHaveAttribute("data-size", "receipt");
    await expect.poll(() => page.locator("style").first().textContent()).not.toContain("297mm");
    const sheetMm = await sheet.evaluate((el) => (el.getBoundingClientRect().height * 25.4) / 96);
    const pdf = await page.pdf({ preferCSSPageSize: true, printBackground: true });
    expect(pagesIn(pdf)).toBe(1);
    const pageMm = pageHeightMm(pdf);
    expect(pageMm).toBeGreaterThanOrEqual(sheetMm);
    expect(pageMm).toBeLessThan(sheetMm + 6);
    expect(pageMm).toBeLessThan(450);
    // Every piece of text on the receipt is at least 9 pt (12 px).
    const smallest = await sheet.evaluate((root) => {
      let least = Number.POSITIVE_INFINITY;
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (!node.textContent?.trim() || !node.parentElement) continue;
        least = Math.min(least, Number.parseFloat(getComputedStyle(node.parentElement).fontSize));
      }
      return least;
    });
    expect(smallest).toBeGreaterThanOrEqual(12);
    await page.screenshot({ path: `${SHOTS}/print-receipt.png`, fullPage: true });
  });

  test("the preview shrinks to fit a narrow window instead of scrolling sideways", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 400, height: 760 });
    await ready(page);
    for (const size of [/A4/, /Letter/, /Receipt/]) {
      await page.getByRole("radio", { name: size }).check();
      await expect
        .poll(() =>
          page.evaluate(
            () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
          ),
        )
        .toBeLessThanOrEqual(0);
      const box = await page.getByRole("article", { name: "Print" }).boundingBox();
      expect(box?.x ?? 0).toBeGreaterThanOrEqual(0);
      expect((box?.x ?? 0) + (box?.width ?? 0)).toBeLessThanOrEqual(400);
    }
    await page.getByRole("radio", { name: /A4/ }).check();
    await page.screenshot({ path: `${SHOTS}/print-narrow.png` });
  });

  test("the sheet sits inside a main landmark and passes axe", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("main", { name: "Print" })).toBeVisible();
    expect(await axe(page)).toEqual([]);
  });

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
