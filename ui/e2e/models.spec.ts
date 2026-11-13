import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const open = async (page: Page, path: string, scenario = "standard") => {
  await page.goto(`/?mockModels=${scenario}#${path}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) => {
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  expect(results.violations.map((v) => v.id)).toEqual([]);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

for (const scheme of ["light", "dark"] as const) {
  test.describe(`models in ${scheme}`, () => {
    test.use({ colorScheme: scheme, locale: "en-GB", timezoneId: "Europe/London" });

    test("axe passes on the page and on a banner of each kind", async ({ page }) => {
      await open(page, "/settings/models");
      await page.getByText("Up to 10.6 GB of the 19.3 GB Nocturne allows itself").waitFor();
      await axe(page);
      await page.screenshot({ path: `${SHOTS}/models-${scheme}.png` });
      for (const scenario of ["downloading", "attention", "reindex", "oldKeep"]) {
        await open(page, "/settings/models", scenario);
        await page.getByRole("link", { name: "Change Answering questions" }).waitFor();
        await axe(page);
      }
    });

    test("axe passes on the picker, a reason opened in place, and the confirm", async ({
      page,
    }) => {
      await open(page, "/settings/models/answering");
      await page.getByRole("radio", { name: /Qwen3-VL-8B/ }).waitFor();
      await axe(page);
      await page.getByRole("button", { name: "Why?" }).first().click();
      await page.getByRole("region", { name: "Why this can’t be chosen" }).waitFor();
      await axe(page);
      await page.screenshot({ path: `${SHOTS}/models-pick-${scheme}.png`, fullPage: true });
      await page.getByRole("radio", { name: /Qwen3-VL-8B/ }).check();
      await page.getByRole("button", { name: "Use the chosen one…" }).click();
      await page.getByRole("dialog", { name: /Get Qwen3-VL-8B/ }).waitFor();
      await axe(page);
    });
  });
}

test("the picker works from the keyboard and Escape gives focus back", async ({ page }) => {
  await open(page, "/settings/models/recordings");
  const current = page.getByRole("radio", { name: /Whisper-Small/ });
  await current.focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("radio", { name: /Whisper-Large-v3-Turbo/ })).toBeChecked();
  const use = page.getByRole("button", { name: "Use the chosen one…" });
  await use.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: /Use Whisper-Large-v3-Turbo/ });
  await dialog.waitFor();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(use).toBeFocused();
});

test("a download is asked for, runs, and the old one is kept by default", async ({ page }) => {
  await open(page, "/settings/models/answering");
  await page.getByRole("radio", { name: /Qwen3-VL-8B/ }).check();
  await page.getByRole("button", { name: "Use the chosen one…" }).click();
  const dialog = page.getByRole("dialog", { name: /Get Qwen3-VL-8B/ });
  await expect(dialog).toContainText("6.2 GB, once");
  await expect(dialog).toContainText("220 GB now, about 214 GB after");
  await dialog.getByRole("button", { name: "Download 6.2 GB" }).click();
  await page.getByRole("progressbar", { name: "Getting Qwen3-VL-8B" }).waitFor();
  await expect(
    page.getByText("Qwen3.5-9B-GGUF keeps working until the new one is ready."),
  ).toBeVisible();
  const keep = page.getByRole("button", { name: "Keep it" });
  await keep.waitFor({ timeout: 15_000 });
  await expect(keep).toBeFocused();
  await expect(page.getByText("Qwen3-VL-8B is now answering questions.")).toBeVisible();
});

test("the online option is only in Answering and hands off to its setup", async ({ page }) => {
  await open(page, "/settings/models/finding");
  await page.getByRole("radio").first().waitFor();
  await expect(page.getByText("Online answers")).toHaveCount(0);
  await open(page, "/settings/models/answering");
  await page.getByRole("radio", { name: /Online answers/ }).check();
  await expect(page.getByRole("link", { name: "Set up online answers…" })).toHaveAttribute(
    "href",
    "#/settings/online/setup",
  );
});

test.describe("narrow", () => {
  test.use({ viewport: { width: 400, height: 760 } });
  test("400 wide has no sideways scroll", async ({ page }) => {
    await open(page, "/settings/models", "attention");
    await page.getByRole("link", { name: "Change Answering questions" }).waitFor();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await open(page, "/settings/models/answering");
    await page.getByRole("radio", { name: /Qwen3-VL-8B/ }).waitFor();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/models-pick-narrow.png`, fullPage: true });
  });
});
