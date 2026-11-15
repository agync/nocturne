import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page, hash = "#/fading") => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1, name: "Fading & let go" }).waitFor();
  await page.getByText("Ava’s old swimming teacher is called Jo.").waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("screenshots of #/fading", () => {
  test("light", async ({ page }) => {
    await ready(page);
    await page
      .getByRole("checkbox", { name: "The old gym door code is 4821." })
      .check({ force: true });
    await page
      .getByRole("checkbox", { name: "The pizza place on Elm Street is good." })
      .check({ force: true });
    await page.screenshot({ path: `${SHOTS}/fading-light.png` });
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
    await page.screenshot({ path: `${SHOTS}/fading-dark.png` });
    await context.close();
  });

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("400 wide", async ({ page }) => {
      await ready(page);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      await page.screenshot({ path: `${SHOTS}/fading-narrow.png` });
    });
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on fading in ${scheme}, also with a selection`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    await ready(page);
    let results = await runAxe(page, (b) => b.withTags(TAGS));
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await page
      .getByRole("checkbox", { name: "The old gym door code is 4821." })
      .check({ force: true });
    results = await runAxe(page, (b) => b.withTags(TAGS));
    expect(results.violations.map((v) => v.id)).toEqual([]);
    await context.close();
  });
}

test.describe("using the list", () => {
  test("Keep it works from the keyboard and focus moves to the next row", async ({ page }) => {
    await ready(page);
    const first = page.getByRole("button", {
      name: "Keep it: Ava’s old swimming teacher is called Jo.",
    });
    await first.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept. It will not fade for 30 days.")).toBeVisible();
    await expect(page.getByText("Ava’s old swimming teacher is called Jo.")).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Keep it: Bin day moved to Tuesdays over summer." }),
    ).toBeFocused();
  });

  test("rows leave at once when motion is reduced", async ({ browser }) => {
    const context = await browser.newContext({ reducedMotion: "reduce", locale: "en-GB" });
    const page = await context.newPage();
    await ready(page);
    await page
      .getByRole("button", { name: "Bring back: The pizza place on Elm Street is good." })
      .click();
    await expect(page.getByText("The pizza place on Elm Street is good.")).toHaveCount(0, {
      timeout: 300,
    });
    await context.close();
  });

  test("bringing back a selection counts them and removes them", async ({ page }) => {
    await ready(page);
    await page
      .getByRole("checkbox", { name: "The old gym door code is 4821." })
      .check({ force: true });
    await page
      .getByRole("checkbox", { name: "The pizza place on Elm Street is good." })
      .check({ force: true });
    await expect(page.getByRole("heading", { name: "Let go · 2 selected" })).toBeVisible();
    await page.getByRole("button", { name: "Bring back 2" }).click();
    await expect(page.getByText(/Brought back 2 things/)).toBeVisible();
    await expect(page.getByText("The old gym door code is 4821.")).toHaveCount(0);
    await expect(page.getByText("Nothing has been let go.")).toBeVisible();
  });

  test("?show=letGo lands on the let-go list", async ({ page }) => {
    await page.goto("/#/fading?show=letGo");
    await page.getByRole("heading", { name: /^Let go · 2/ }).waitFor();
    await expect(page.getByRole("heading", { name: /^Let go · 2/ })).toBeFocused();
  });
});

test.describe("text spacing at 400 px (WCAG 1.4.12)", () => {
  test.use({ viewport: { width: 400, height: 800 } });

  test("no row hides any of its text or its button", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({
      content: `* { line-height: 1.5 !important; letter-spacing: 0.12em !important;
        word-spacing: 0.16em !important; }
        p { margin-bottom: 2em !important; }`,
    });
    // A long sentence, as a person's own words can be.
    await page
      .locator("li[data-row-id] span[class*=sentence]")
      .first()
      .evaluate((el) => {
        el.textContent = `${"Everything the council said about the garden fence and the bins. ".repeat(5)}`;
      });
    const clipped = await page
      .locator("li[data-row-id]")
      .evaluateAll((rows) =>
        rows
          .filter((row) => row.scrollHeight > row.clientHeight + 1)
          .map((row) => row.textContent?.slice(0, 30)),
      );
    expect(clipped).toEqual([]);
    const first = page.locator("li[data-row-id]").first();
    const row = await first.boundingBox();
    const button = await first.getByRole("button").boundingBox();
    expect((button?.y ?? 0) + (button?.height ?? 0)).toBeLessThanOrEqual(
      (row?.y ?? 0) + (row?.height ?? 0) + 1,
    );
    expect(await overflow(page)).toBeLessThanOrEqual(0);
  });
});
