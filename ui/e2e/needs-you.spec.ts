import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page) => {
  await page.goto("/#/needs-you");
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

const rows = (page: Page) =>
  page.getByRole("list", { name: "Things waiting for you" }).locator("> li");

const said = (page: Page) => page.locator("p[class*=said]");

test.describe("Needs you", () => {
  test("says how many are waiting and how to move", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("2 things are waiting");
    await expect(page.getByText("Needs you · 2")).toBeVisible();
    await expect(page.getByText("J / K to move · 1 2 3 to choose")).toBeVisible();
    await expect(rows(page)).toHaveCount(2);
    await page.screenshot({ path: `${SHOTS}/needs-you-light.png` });
  });

  test("choosing the right dentist time by pointer says so and lowers the count in the rail", async ({
    page,
  }) => {
    await ready(page);
    await page.getByRole("button", { name: /^This one: Thursday/ }).click();
    await expect(said(page)).toContainText(
      /^Dentist set to Thursday .* The other is kept as history\.$/,
    );
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("One thing is waiting");
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: /Needs you/ }),
    ).toContainText("1");
    await expect(said(page)).toHaveAttribute("aria-live", "polite");
  });

  test("keys: J and K move, 1 and 2 choose, and focus lands on the next entry", async ({
    page,
  }) => {
    await ready(page);
    await rows(page).nth(0).focus();
    await page.keyboard.press("j");
    await expect(rows(page).nth(1)).toBeFocused();
    await page.keyboard.press("k");
    await expect(rows(page).nth(0)).toBeFocused();
    await page.keyboard.press("2");
    await expect(said(page)).toContainText("Dentist set to Thursday");
    await expect(rows(page)).toHaveCount(1);
    await expect(rows(page).nth(0)).toBeFocused();
    await page.keyboard.press("1");
    await expect(page.getByText("Nothing needs you.")).toBeVisible();
    await expect(
      page.locator("[role=status]").filter({ hasText: "Nothing needs you." }),
    ).toBeFocused();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("All clear");
    await page.screenshot({ path: `${SHOTS}/needs-you-clear-light.png` });
  });

  test("the keys do nothing while focus is outside the list", async ({ page }) => {
    await ready(page);
    await page.getByRole("main").focus();
    await page.keyboard.press("2");
    await page.keyboard.press("j");
    await expect(rows(page)).toHaveCount(2);
  });

  test("everything can be reached and used with Tab and Enter", async ({ page }) => {
    await ready(page);
    await page.getByRole("main").focus();
    const reached: string[] = [];
    for (let i = 0; i < 6; i += 1) {
      await page.keyboard.press("Tab");
      reached.push(
        await page.evaluate(() =>
          (
            document.activeElement?.getAttribute("aria-label") ??
            document.activeElement?.textContent ??
            ""
          )
            .trim()
            .slice(0, 40),
        ),
      );
    }
    for (const word of ["This one: Tuesday", "This one: Thursday", "Done:"]) {
      expect(
        reached.some((r) => r.includes(word)),
        word,
      ).toBe(true);
    }
    await page.getByRole("button", { name: /^Done:/ }).focus();
    await page.keyboard.press("Enter");
    await expect(said(page)).toContainText("done.");
    await expect(rows(page)).toHaveCount(1);
  });

  test("a receipt opens its original", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: /^Appointment card/ }).click();
    await expect(page).toHaveURL(/#\/items\/item-card$/);
  });

  test("200% text reflows without sideways scrolling", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
    await page.waitForTimeout(200);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/needs-you-text-200.png` });
  });
});

test.describe("a narrow window", () => {
  test.use({ viewport: { width: 400, height: 760 } });

  test("stacks the two sources and never scrolls sideways", async ({ page }) => {
    await ready(page);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    const cards = page.getByRole("button", { name: /^This one:/ });
    const [a, b] = [await cards.nth(0).boundingBox(), await cards.nth(1).boundingBox()];
    expect((b?.y ?? 0) > (a?.y ?? 0) + (a?.height ?? 0) - 1).toBe(true);
    await page.screenshot({ path: `${SHOTS}/needs-you-narrow.png` });
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes before, during and after in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await ready(page);
    expect(await axe(page), "waiting").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/needs-you-dark.png` });
    await page.getByRole("button", { name: /^This one: Tuesday/ }).click();
    await expect(said(page)).toContainText("Dentist set to");
    expect(await axe(page), "one left").toEqual([]);
    await page.getByRole("button", { name: /^Done:/ }).click();
    await expect(page.getByText("Nothing needs you.")).toBeVisible();
    expect(await axe(page), "all clear").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/needs-you-clear-dark.png` });
    await context.close();
  });
}
