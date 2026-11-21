import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// The morning page and the journal day with Sam's phone paired: the calendar, last night, the
// timeline rows and the From Health block, at 1280 wide, light and dark, with axe.

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const schemes = ["light", "dark"] as const;

for (const scheme of schemes) {
  test.describe(`with a phone's calendar and health, ${scheme}`, () => {
    test.use({ colorScheme: scheme, viewport: { width: 1280, height: 900 } });

    const ready = async (page: Page, hash: string) => {
      await page.goto(`/?mockPhones=1${hash}`);
      await page.getByRole("heading", { level: 1 }).first().waitFor();
      await page.evaluate(() => document.fonts.ready);
    };

    test("the morning page shows last night and what is on today", async ({ page }) => {
      await ready(page, "#/today");
      await expect(page.getByRole("heading", { name: "Last night" })).toBeVisible();
      await expect(page.getByText("6 h 40 min asleep, 11:50 pm to 7:05 am")).toBeVisible();
      await expect(
        page.getByRole("img", { name: "6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once." }),
      ).toBeVisible();
      await expect(page.getByRole("heading", { name: "In your calendar today · 3" })).toBeVisible();
      await expect(page.getByText("Dentist, check-up")).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-today-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
    });

    test("a receipt on the calendar opens the entry's own page", async ({ page }) => {
      await ready(page, "#/today");
      await page
        .getByRole("button", { name: "Calendar · Family · Sam’s phone. Source." })
        .first()
        .click();
      await expect(
        page.getByRole("heading", { level: 1, name: "Dentist, check-up" }),
      ).toBeVisible();
      await expect(
        page.getByText("From Sam’s phone’s calendar · last updated 7:42 am"),
      ).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-item-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
    });

    test("the journal day has the rows, the side block and Show as a list", async ({ page }) => {
      await ready(page, "&mockJournal=day#/journal");
      await expect(page.getByText("Woke · 6 h 40 min asleep")).toBeVisible();
      await expect(page.getByText("Walk · 42 min · 3.1 km")).toBeVisible();
      await expect(page.getByText("from your calendar").first()).toBeVisible();
      await expect(page.getByRole("heading", { name: "From Health" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-journal-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
    });

    test("Delete this day lists the calendar and the readings, unticked", async ({ page }) => {
      await ready(page, "&mockJournal=day#/journal");
      await page.getByRole("button", { name: "About this day" }).click();
      await page.getByRole("menuitem", { name: "Delete this day…" }).click();
      const dialog = page.getByRole("dialog");
      await expect(dialog.getByRole("region", { name: "From your calendar" })).toBeVisible();
      await expect(dialog.getByRole("region", { name: "From Health" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-delete-day-${scheme}.png` });
      const results = await runAxe(page, (b) => b.withTags(TAGS));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
    });
  });
}
