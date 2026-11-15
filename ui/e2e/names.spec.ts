import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

/** WCAG 2.5.3: a control's name contains the words drawn on it. Axe keeps this rule experimental. */
const mismatches = async (page: Page) =>
  (await runAxe(page, (b) => b.withRules(["label-content-name-mismatch"]))).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const SCREENS = [
  "#/today",
  "#/pages/2026-11-04",
  "#/pages/2026-11-05",
  "#/needs-you",
  "#/recent",
  "#/memories",
  "#/memories/ent-leo",
  "#/items/item-letter",
  "#/items/item-lake",
  "#/items/item-nut",
  "#/fading",
  "#/pages",
  "#/privacy",
  "#/settings",
  "#/settings/advanced",
  "#/print/2026-11-13",
];

test.describe("every visible label is in the accessible name", () => {
  for (const hash of SCREENS) {
    test(hash, async ({ page }) => {
      await page.goto(`/${hash}`);
      await page.getByRole("heading", { level: 1 }).first().waitFor();
      await page.evaluate(() => document.fonts.ready);
      expect(await mismatches(page)).toEqual([]);
    });
  }

  test("an answer and its receipts", async ({ page }) => {
    await page.goto("/#/today");
    await page
      .getByRole("textbox", { name: "Keep something, or ask" })
      .fill("What is Gran’s new number?");
    await page.keyboard.press("Enter");
    await expect(page.locator("article[aria-busy='false']")).toBeVisible();
    await expect(page.getByRole("button", { name: /Note · / })).toBeVisible();
    expect(await mismatches(page)).toEqual([]);
  });
});
