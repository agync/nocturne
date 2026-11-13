import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const SCREENS: Array<[string, string, string]> = [
  ["settings", "#/settings", "Settings"],
  ["advanced", "#/settings/advanced", "Advanced details"],
];

const ready = async (page: Page, hash: string, heading: string) => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1, name: heading }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("screenshots", () => {
  for (const [name, hash, heading] of SCREENS) {
    test(`${name} light`, async ({ page }) => {
      await ready(page, hash, heading);
      await page.screenshot({ path: `${SHOTS}/${name}-light.png` });
    });

    test(`${name} dark`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: "dark",
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1280, height: 800 },
      });
      const page = await context.newPage();
      await ready(page, hash, heading);
      await page.screenshot({ path: `${SHOTS}/${name}-dark.png` });
      await context.close();
    });

    test.describe(`${name} narrow`, () => {
      test.use({ viewport: { width: 400, height: 760 } });
      test("400 wide", async ({ page }) => {
        await ready(page, hash, heading);
        expect(await overflow(page)).toBeLessThanOrEqual(0);
        await page.screenshot({ path: `${SHOTS}/${name}-narrow.png`, fullPage: true });
      });
    });
  }
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on settings and advanced in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    for (const [, hash, heading] of SCREENS) {
      await ready(page, hash, heading);
      const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
      expect(results.violations.map((v) => `${hash} ${v.id}`)).toEqual([]);
    }
    await ready(page, "#/settings", "Settings");
    await page.getByRole("button", { name: "Change quick keep shortcut" }).click();
    const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await context.close();
  });
}

test.describe("using it", () => {
  test("a switch saves from the keyboard and says Saved.", async ({ page }) => {
    await ready(page, "#/settings", "Settings");
    const login = page.getByRole("switch", { name: "Start with this computer" });
    const was = await login.getAttribute("aria-checked");
    await login.focus();
    await page.keyboard.press("Space");
    await expect(login).toHaveAttribute("aria-checked", was === "true" ? "false" : "true");
    await expect(page.getByText("Saved.")).toBeVisible();
  });

  test("the theme setting changes the colours at once", async ({ page }) => {
    await ready(page, "#/settings", "Settings");
    await page.getByRole("combobox", { name: "Theme · language, theme" }).selectOption("dark");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  });

  test("the shortcut field lets Tab and Esc out and takes a new combination", async ({ page }) => {
    await ready(page, "#/settings", "Settings");
    await page.getByRole("button", { name: "Change quick keep shortcut" }).click();
    const field = page.getByRole("textbox", { name: /press the new keys/ });
    await expect(field).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Change quick keep shortcut" })).toBeFocused();
    await page.getByRole("button", { name: "Change quick keep shortcut" }).click();
    await page.keyboard.press("Tab");
    await expect(field).toHaveCount(0);
    await page.getByRole("button", { name: "Change quick keep shortcut" }).click();
    await page.keyboard.press("Control+Alt+K");
    await expect(page.getByText("Ctrl+Alt+K")).toBeVisible();
  });

  test("advanced details are one link away and one link back", async ({ page }) => {
    await ready(page, "#/settings", "Settings");
    await page.getByRole("link", { name: "Show advanced details" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Advanced details" })).toBeVisible();
    await expect(page.getByText("Lemonade 8.2.2", { exact: false })).toBeVisible();
    await page.getByRole("link", { name: "Back to settings" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Settings" })).toBeVisible();
  });

  test("Reset Nocturne asks for the phrase and goes back to the welcome", async ({ page }) => {
    await ready(page, "#/settings/advanced", "Advanced details");
    await page.getByRole("button", { name: "Reset Nocturne…" }).click();
    const dialog = page.getByRole("dialog", { name: "Reset Nocturne?" });
    await dialog.getByRole("textbox").fill("delete everything");
    await dialog.getByRole("button", { name: "Reset Nocturne" }).click();
    await expect(page).toHaveURL(/#\/setup\?step=welcome$/);
  });
});
