import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const GYM = "The old gym door code is 4821.";

const openDialog = async (page: Page) => {
  await page.goto("/#/fading");
  await page.getByRole("heading", { level: 1, name: "Fading & let go" }).waitFor();
  await page.evaluate(() => document.fonts.ready);
  await page.getByRole("checkbox", { name: GYM }).check({ force: true });
  await page.getByRole("button", { name: /^(Forget 1 completely|Delete 1)…/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: /^Delete 2 things/ }).waitFor();
  return dialog;
};

test.describe("forgetting completely", () => {
  test("names the memory and the original, with the count on the button", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(dialog.getByText(GYM)).toBeVisible();
    await expect(dialog.getByRole("checkbox", { name: "Also delete the original" })).toBeChecked();
    await expect(dialog.getByText(/exported or printed is out of reach/)).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/forget-light.png` });
  });

  test("focus starts on Keep them and Enter does not forget", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(dialog.getByRole("button", { name: /^Keep (it|them)$/ })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog).toHaveCount(0);
    await expect(page.getByText(GYM)).toBeVisible();
  });

  test("Escape closes it, Tab stays inside, and focus goes back", async ({ page }) => {
    const dialog = await openDialog(page);
    for (let i = 0; i < 8; i += 1) {
      await page.keyboard.press("Tab");
      expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
    }
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: /^(Forget 1 completely|Delete 1)…/ }),
    ).toBeFocused();
  });

  test("the forget button works with the keyboard and the row leaves", async ({ page }) => {
    const dialog = await openDialog(page);
    await dialog.getByRole("button", { name: /^Delete 2 things/ }).focus();
    await page.keyboard.press("Enter");
    await expect(dialog.getByText("Deleted.", { exact: true })).toBeVisible();
    await expect(dialog.getByRole("status")).toContainText("They won’t appear");
    await page.screenshot({ path: `${SHOTS}/forget-done-light.png` });
    await dialog.getByRole("button", { name: "OK" }).click();
    await expect(page.getByText(GYM)).toHaveCount(0);
    await expect(page.getByText("The pizza place on Elm Street is good.")).toBeVisible();
  });

  test("keeping the original is one tick away", async ({ page }) => {
    const dialog = await openDialog(page);
    await dialog
      .getByRole("checkbox", { name: "Also delete the original" })
      .uncheck({ force: true });
    await expect(dialog.getByRole("button", { name: "Delete 1 memory" })).toBeVisible();
    await expect(dialog.getByText(/so does its transcript/)).toBeVisible();
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`axe passes on the dialog in ${scheme}`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: scheme,
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1280, height: 800 },
      });
      const page = await context.newPage();
      await openDialog(page);
      if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/forget-dark.png` });
      const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
      expect(results.violations.map((v) => v.id)).toEqual([]);
      await context.close();
    });
  }

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("fits at 400 px", async ({ page }) => {
      const dialog = await openDialog(page);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      ).toBeLessThanOrEqual(0);
      await expect(dialog.getByRole("button", { name: /^Keep (it|them)$/ })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/forget-narrow.png` });
    });
  });
});
