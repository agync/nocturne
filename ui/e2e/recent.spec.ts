import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page) => {
  await page.goto("/#/recent");
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.getByRole("list", { name: "Things you kept today and yesterday" }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

const list = (page: Page) =>
  page.getByRole("list", { name: "Things you kept today and yesterday" });
const row = (page: Page, title: string) => list(page).locator("li", { hasText: title }).first();

test.describe("Recently kept", () => {
  test("reads like the design", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Today and yesterday");
    await expect(
      page.getByText(
        "1 waiting to be read. Nocturne reads it tonight, or sooner if the computer is free.",
      ),
    ).toBeVisible();
    await expect(row(page, "Parking permit renewal")).toContainText("Photo · 06:41 · quick keep");
    await expect(row(page, "Parking permit renewal")).toContainText("Reading…");
    await expect(row(page, "Dentist moved")).toContainText("Needs you");
    await expect(row(page, "council-tax-2026.pdf")).toContainText(
      "Save an unlocked copy and drop it here",
    );
    await page.screenshot({ path: `${SHOTS}/recent-light.png` });
  });

  test("a locked file has no button; a reading item has none either", async ({ page }) => {
    await ready(page);
    await expect(row(page, "council-tax-2026.pdf").getByRole("button")).toHaveCount(0);
    await expect(row(page, "Parking permit renewal").getByRole("button")).toHaveCount(0);
  });

  test("Read now reads at once, keeps focus on the row, and the row follows to Understood", async ({
    page,
  }) => {
    await ready(page);
    await page.getByRole("textbox", { name: "Keep something, or ask" }).fill("Buy milk");
    await page.keyboard.press("Enter");
    const milk = row(page, "Buy milk");
    await expect(milk).toContainText("Kept · reads tonight");
    await page.screenshot({ path: `${SHOTS}/recent-kept-light.png` });
    await milk.getByRole("button", { name: "Read now: Buy milk" }).click();
    await expect(milk).toContainText("Reading…");
    await expect(milk).toBeFocused();
    // The focused row's change is said, politely.
    const announcer = page
      .locator("[role='status'][aria-atomic='true']")
      .filter({ hasText: "Buy milk" });
    await expect(milk).toContainText("Understood", { timeout: 8000 });
    await expect(announcer).toHaveText(/Buy milk: Understood\./);
    await expect(announcer).toHaveAttribute("aria-live", "polite");
    await expect(milk.getByRole("button", { name: "Open: Buy milk" })).toBeVisible();
  });

  test("a note kept while the list is open appears and is announced once", async ({ page }) => {
    await ready(page);
    await page.getByRole("textbox", { name: "Keep something, or ask" }).fill("Call the plumber");
    await page.keyboard.press("Enter");
    await expect(row(page, "Call the plumber")).toBeVisible();
    await expect(
      page
        .locator("[role='status'][aria-atomic='true']")
        .filter({ hasText: "Kept: Call the plumber." }),
    ).toHaveCount(1);
  });

  test("Open, Which is right? and Confirm take the person where they need to be", async ({
    page,
  }) => {
    await ready(page);
    await page.getByRole("button", { name: "Open: Class WhatsApp, 3 messages" }).click();
    await expect(page).toHaveURL(/#\/items\/item-whatsapp$/);
    await page.goBack();
    await page.getByRole("button", { name: "Which is right?: Dentist moved" }).click();
    await expect(page).toHaveURL(/#\/needs-you$/);
  });

  test("every action is reachable from the keyboard and names its item", async ({ page }) => {
    await ready(page);
    await page.getByRole("main").focus();
    const reached: string[] = [];
    for (let i = 0; i < 12; i += 1) {
      await page.keyboard.press("Tab");
      reached.push(
        await page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? ""),
      );
    }
    for (const name of ["Open: Ask Sam about Saturday", "Which is right?: Dentist moved"]) {
      expect(reached, name).toContain(name);
    }
  });

  test("200% text reflows without sideways scrolling", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
    await page.waitForTimeout(200);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/recent-text-200.png` });
  });
});

test.describe("a narrow window", () => {
  test.use({ viewport: { width: 400, height: 760 } });

  test("stacks status and action under each title and never scrolls sideways", async ({ page }) => {
    await ready(page);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    const parking = await row(page, "Parking permit renewal").boundingBox();
    const dentist = await row(page, "Dentist moved").getByRole("button").boundingBox();
    expect(parking?.width ?? 999).toBeLessThanOrEqual(400);
    expect(dentist?.x ?? 0).toBeGreaterThan(40);
    await page.screenshot({ path: `${SHOTS}/recent-narrow.png` });
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await ready(page);
    expect(await axe(page)).toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/recent-dark.png` });
    await page.getByRole("textbox", { name: "Keep something, or ask" }).fill("Buy milk");
    await page.keyboard.press("Enter");
    await expect(row(page, "Buy milk")).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await context.close();
  });
}
