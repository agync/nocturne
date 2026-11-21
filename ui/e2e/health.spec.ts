import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// The health pages with Sam's phone paired, at 1280 wide, light and dark, with axe.

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const KINDS: Array<[slug: string, title: string]> = [
  ["sleep", "Sleep"],
  ["heart-rate", "Heart rate"],
  ["resting-heart-rate", "Resting heart rate"],
  ["blood-pressure", "Blood pressure"],
  ["steps", "Steps"],
  ["workouts", "Workouts"],
];

for (const scheme of ["light", "dark"] as const) {
  test.describe(`health pages, ${scheme}`, () => {
    test.use({ colorScheme: scheme, viewport: { width: 1280, height: 900 } });

    const ready = async (page: Page, slug: string) => {
      await page.goto(`/?mockPhones=1#/journal/health/${slug}`);
      await page.getByRole("heading", { level: 1 }).first().waitFor();
      await page.evaluate(() => document.fonts.ready);
    };

    for (const [slug, title] of KINDS) {
      test(`${title} has its chart, sentence, fixed line and no violations`, async ({ page }) => {
        await ready(page, slug);
        await expect(page.getByRole("heading", { level: 1, name: title })).toBeVisible();
        await expect(
          page.getByText(
            "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can.",
          ),
        ).toBeVisible();
        if (slug !== "workouts") await expect(page.getByRole("img").first()).toBeVisible();
        await page.screenshot({ path: `${SHOTS}/health-${slug}-${scheme}.png` });
        const results = await runAxe(page, (b) => b.withTags(TAGS));
        expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
      });
    }

    test("Show as a list gives the numbers by day", async ({ page }) => {
      await ready(page, "sleep");
      await page.getByRole("button", { name: "Show as a list" }).click();
      await expect(page.getByRole("table").first()).toBeVisible();
      await page.getByRole("button", { name: "Show as a chart" }).click();
      await expect(page.getByRole("img").first()).toBeVisible();
    });

    test("a day's chart point and list value lead to that day, and the day is marked", async ({
      page,
    }) => {
      await ready(page, "steps");
      await page.locator('[data-chart="bars"] + svg a').nth(3).click();
      await expect(page).toHaveURL(/#\/journal\/health\/steps\?day=\d{4}-\d{2}-\d{2}$/);
      await expect(page.locator('li[aria-current="true"]')).toHaveCount(1);
      await page.getByRole("button", { name: "Show as a list" }).click();
      await expect(page.locator('tr[aria-current="true"]')).toHaveCount(1);
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/health-day-${scheme}.png` });
    });

    test("a reading removed in Health is not in the charts", async ({ page }) => {
      await ready(page, "sleep");
      await expect(page.getByText("Removed in Health · not in charts")).toBeVisible();
    });

    test("a reading's menu deletes that reading alone", async ({ page }) => {
      await ready(page, "steps");
      await page
        .getByRole("button", { name: /^More about the reading on/ })
        .first()
        .click();
      await page.getByRole("menuitem", { name: "Delete…" }).click();
      const dialog = page.getByRole("dialog", { name: "Delete this reading?" });
      await expect(dialog.getByText("1 reading of steps from Sam’s phone")).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/health-delete-one-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
      await dialog.getByRole("button", { name: "Keep them" }).click();
      await expect(dialog).toBeHidden();
    });

    test("Delete readings previews before it deletes", async ({ page }) => {
      await ready(page, "sleep");
      await page.getByRole("button", { name: "Delete readings…" }).click();
      const dialog = page.getByRole("dialog");
      await expect(dialog.getByText(/readings? of sleep from Sam’s phone/)).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/health-delete-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
      await dialog.getByRole("button", { name: "Keep them" }).click();
      await expect(dialog).toBeHidden();
    });
  });
}
