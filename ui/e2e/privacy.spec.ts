import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page) => {
  await page.goto("/#/privacy");
  await page.getByRole("heading", { level: 1, name: "Only downloads you agreed to." }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("screenshots of #/privacy", () => {
  test("light", async ({ page }) => {
    await ready(page);
    await page.screenshot({ path: `${SHOTS}/privacy-light.png` });
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
    await page.screenshot({ path: `${SHOTS}/privacy-dark.png` });
    await context.close();
  });

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("400 wide", async ({ page }) => {
      await ready(page);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      await page.screenshot({ path: `${SHOTS}/privacy-narrow.png`, fullPage: true });
    });
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on privacy in ${scheme}, with the delete dialog open`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    await ready(page);
    let results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await page.getByRole("button", { name: "Delete everything…" }).click();
    await page.getByRole("dialog", { name: "Delete everything?" }).waitFor();
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/privacy-delete-dark.png` });
    results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await context.close();
  });
}

test.describe("using it", () => {
  test("exporting asks for a folder, shows where it went and can open it", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Export to a folder…" }).click();
    await expect(page.getByText("Exported to Chosen folder/Nocturne export.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Open export folder" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Export to a folder…" })).toBeFocused();
    await page.screenshot({ path: `${SHOTS}/privacy-export-light.png` });
  });

  test("there is no lock switch and no update checks to promise", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("switch")).toHaveCount(0);
    await expect(page.getByText(/sign-in|checking for updates/i)).toHaveCount(0);
  });

  test("delete everything needs the phrase, then goes back to the welcome", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Delete everything…" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete everything?" });
    const field = dialog.getByRole("textbox");
    await expect(field).toBeFocused();
    const confirm = dialog.getByRole("button", { name: "Delete everything" });
    await expect(confirm).toBeDisabled();
    await field.fill("delete everything");
    await field.press("Enter");
    await expect(dialog).toBeVisible();
    await confirm.click();
    await expect(page).toHaveURL(/#\/setup\?step=welcome$/);
  });

  test("Escape leaves the dialog and focus goes back to its button", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Delete everything…" }).click();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Delete everything…" })).toBeFocused();
  });
});
