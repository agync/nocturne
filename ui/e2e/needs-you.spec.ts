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

/**
 * Keeps a pasted web page from the quick-keep window, waits for it to be read, and leaves the app
 * on Needs you. The page stays loaded, so the mock engine remembers it.
 */
const withWebPage = async (page: Page) => {
  await page.goto("/#/quick-capture");
  const box = page.getByRole("textbox", { name: "Keep something, or ask" });
  await box.waitFor();
  await page.evaluate(() => {
    const input = document.querySelector("textarea") as HTMLTextAreaElement;
    const data = new DataTransfer();
    data.setData("text/plain", "Leo is allergic to nuts so I always make nut-free lunches");
    data.setData("text/html", "<p>Leo is allergic to nuts so I always make nut-free lunches</p>");
    input.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
  });
  await box.fill("Leo is allergic to nuts so I always make nut-free lunches");
  await page.keyboard.press("Enter");
  await expect(page.getByText("Kept as unconfirmed. Nocturne will read it tonight.")).toBeVisible();
  await page.waitForTimeout(5200);
  await page.evaluate(() => {
    window.location.hash = "#/needs-you";
  });
  await page.getByRole("heading", { level: 1 }).waitFor();
};

test.describe("an unconfirmed web page", () => {
  test("waits on Needs you with where it came from and the three ways to answer", async ({
    page,
  }) => {
    await withWebPage(page);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("3 things are waiting");
    await expect(page.getByText("Unconfirmed", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Web page you pasted/ })).toBeVisible();
    for (const name of ["It’s true", "Not true, delete it", "Leave unconfirmed"]) {
      await expect(page.getByRole("button", { name })).toBeVisible();
    }
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: /Needs you/ }),
    ).toContainText("3");
    await page.screenshot({ path: `${SHOTS}/needs-you-unconfirmed-light.png` });
    await page.getByRole("button", { name: "It’s true" }).click();
    await expect(said(page)).toContainText("Marked as sure because you said so.");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("2 things are waiting");
  });

  test("Leave unconfirmed removes it and the rail count follows", async ({ page }) => {
    await withWebPage(page);
    await page.getByRole("button", { name: "Leave unconfirmed" }).click();
    await expect(said(page)).toContainText("Left unconfirmed.");
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: /Needs you/ }),
    ).toContainText("2");
  });

  test("Not true, delete it deletes through the shared dialog and focus moves on when it closes", async ({
    page,
  }) => {
    await withWebPage(page);
    await page.getByRole("button", { name: "Not true, delete it" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("button", { name: /^Keep (it|them)$/ })).toBeFocused();
    await dialog.getByRole("button", { name: /^Delete 1 memory$/ }).click();
    await expect(dialog.getByText("Deleted.", { exact: true })).toBeVisible();
    // Behind the dialog the entry has already gone.
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("2 things are waiting");
    await dialog.getByRole("button", { name: "OK" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(said(page)).toContainText("Deleted.");
    await expect(rows(page).nth(1)).toBeFocused();
  });

  test("Recently kept shows it as Unconfirmed and Confirm leads here", async ({ page }) => {
    await withWebPage(page);
    await page.evaluate(() => {
      window.location.hash = "#/recent";
    });
    const row = page
      .getByRole("list", { name: "Things you kept today and yesterday" })
      .locator("li", {
        hasText: "Leo is allergic",
      });
    await expect(row).toContainText("Unconfirmed");
    await page.screenshot({ path: `${SHOTS}/recent-unconfirmed-light.png` });
    await row.getByRole("button", { name: /^Confirm:/ }).click();
    await expect(page).toHaveURL(/#\/needs-you$/);
  });

  test("dark and narrow", async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: "dark",
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await withWebPage(page);
    expect(await axe(page), "dark").toEqual([]);
    await page.screenshot({ path: `${SHOTS}/needs-you-unconfirmed-dark.png` });
    await page.setViewportSize({ width: 400, height: 760 });
    await page.waitForTimeout(200);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/needs-you-unconfirmed-narrow.png` });
    await context.close();
  });
});

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
    for (const word of ["This one: Tuesday", "This one: Thursday", "Mark as done:"]) {
      expect(
        reached.some((r) => r.includes(word)),
        word,
      ).toBe(true);
    }
    await page.getByRole("button", { name: /^Mark as done:/ }).focus();
    await page.keyboard.press("Enter");
    await expect(said(page)).toContainText("done.");
    await expect(rows(page)).toHaveCount(1);
  });

  test("Mark as done can be undone while its toast is up", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: /^Mark as done:/ }).click();
    await expect(rows(page)).toHaveCount(1);
    await expect(page.getByText("Marked as done.")).toBeVisible();
    await page.getByRole("button", { name: "Undo" }).click();
    await expect(rows(page)).toHaveCount(2);
    await expect(page.getByRole("button", { name: /^Mark as done:/ })).toBeVisible();
  });

  test("the title sits at the shared left edge and receipts are chips with a 44 px hit area", async ({
    page,
  }) => {
    await ready(page);
    const box = await page.getByRole("heading", { level: 1 }).boundingBox();
    expect(Math.round(box?.x ?? 0)).toBe(292);
    const chip = page.getByRole("button", { name: /^Appointment card/ });
    const hit = await chip.evaluate((el) => {
      const after = getComputedStyle(el, "::after");
      return [Number.parseFloat(after.width), Number.parseFloat(after.height)];
    });
    expect(hit[0]).toBeGreaterThanOrEqual(44);
    expect(hit[1]).toBeGreaterThanOrEqual(44);
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
    await page.getByRole("button", { name: /^Mark as done:/ }).click();
    await expect(page.getByText("Nothing needs you.")).toBeVisible();
    expect(await axe(page), "all clear").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/needs-you-clear-dark.png` });
    await context.close();
  });
}
