import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// A week's recap with Sam's phone paired: "From Health" and what noticing found, at 1280 wide,
// light and dark, with axe.

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const WEEK =
  "/?mockNow=2026-11-19T21:20:00Z&mockPhones=1&mockJournal=day#/journal/look-back/week/2026-11-09";

for (const scheme of ["light", "dark"] as const) {
  test.describe(`recap with Health, ${scheme}`, () => {
    test.use({ colorScheme: scheme, viewport: { width: 1280, height: 2300 } });

    const ready = async (page: Page, url = WEEK) => {
      await page.goto(url);
      await page.getByRole("heading", { level: 1, name: "Your week" }).waitFor();
      await page.getByRole("heading", { level: 2, name: "How it felt" }).waitFor();
      await page.evaluate(() => document.fonts.ready);
    };

    test("From Health follows How it felt, with its nights, steps and workouts", async ({
      page,
    }) => {
      await ready(page);
      const section = page.getByRole("region", { name: "From Health" });
      await expect(section).toBeVisible();
      await expect(section.getByRole("img", { name: /nights\. Asleep/ })).toBeVisible();
      await expect(section.getByText(/^2 walks\.$/)).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/recap-health-${scheme}.png` });
      // The whole recap, not only the new sections.
      const results = await runAxe(page, (b) => b.withTags(TAGS).include("article"));
      expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
    });

    test("a night's receipt opens that day in Health", async ({ page }) => {
      await ready(page);
      const section = page.getByRole("region", { name: "From Health" });
      await section.getByRole("link", { name: /Walk$/ }).first().click();
      await expect(page).toHaveURL(/#\/journal\/health\?day=\d{4}-\d{2}-\d{2}$/);
    });

    test("noticing is a sentence with chips for both sides, and it can be turned off", async ({
      page,
    }) => {
      await ready(page);
      const lines = page.locator("p", {
        hasText: /^On the \d+ nights|^You took about|^On \d+ of the/,
      });
      await expect(lines.first()).toBeVisible();
      await expect(
        page.getByText(
          "Shown only when it held at least three times. It says what went together, never why.",
        ),
      ).toBeVisible();
      await page.locator("article").screenshot({ path: `${SHOTS}/recap-noticing-${scheme}.png` });
      // The mock keeps its state until the page reloads, so move by the hash.
      await page.evaluate(() => {
        window.location.hash = "#/settings/phones";
      });
      await page.getByRole("switch", { name: "Notice what goes together" }).click();
      await page.evaluate(() => {
        window.location.hash = "#/journal/look-back/week/2026-11-09";
      });
      await page.getByRole("heading", { level: 1, name: "Your week" }).waitFor();
      await expect(
        page.getByText(
          "Shown only when it held at least three times. It says what went together, never why.",
        ),
      ).toHaveCount(0);
    });
  });
}
