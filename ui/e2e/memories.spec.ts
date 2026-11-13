import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page, hash: string, heading: string | RegExp = /./) => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1, name: heading }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

const SCREENS: Array<[string, string, string | RegExp]> = [
  ["memories-list", "#/memories", "What Nocturne remembers"],
  ["memories-entity", "#/memories/ent-leo", "Leo"],
  ["memories-search", "#/memories?q=swim", "What Nocturne remembers"],
  ["item-photo", "#/items/item-letter", "Letter"],
  ["item-voice", "#/items/item-lake", "Lake District"],
  ["item-web", "#/items/item-nut", "Nut-free lunchbox ideas"],
];

test.describe("screenshots", () => {
  for (const [name, hash, heading] of SCREENS) {
    test(`${name} light`, async ({ page }) => {
      await ready(page, hash, heading);
      if (name === "memories-entity") {
        await page.getByRole("button", { name: "Leo is allergic to nuts." }).click();
      }
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
        await page.screenshot({ path: `${SHOTS}/${name}-narrow.png` });
      });
    });
  }
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on the memory screens in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    for (const [, hash, heading] of SCREENS) {
      await ready(page, hash, heading);
      if (hash === "#/memories/ent-leo") {
        await page.getByRole("button", { name: "Leo is allergic to nuts." }).click();
        await page.getByRole("button", { name: "Correct" }).click();
      }
      const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
      expect(
        results.violations.map(
          (v) => `${hash} ${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
        ),
      ).toEqual([]);
    }
    await context.close();
  });
}

test.describe("keyboard", () => {
  test("a person can be reached, a memory selected and corrected without a mouse", async ({
    page,
  }) => {
    await ready(page, "#/memories", "What Nocturne remembers");
    await page.getByRole("tab", { name: "People" }).focus();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: /^Leo/ })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/memories\/ent-leo$/);
    await page.getByRole("heading", { level: 1, name: "Leo" }).waitFor();

    const row = page.getByRole("button", { name: "Leo is allergic to nuts." });
    await row.focus();
    await page.keyboard.press("Enter");
    await expect(row).toHaveAttribute("aria-expanded", "true");
    await page.getByRole("button", { name: "Correct" }).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("textbox", { name: "What should it say?" })).toBeFocused();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("Leo is allergic to peanuts.");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Corrected.")).toBeVisible();
    await expect(page.getByText("Leo is allergic to peanuts.", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Leo is allergic to peanuts." })).toBeFocused();
  });

  test("Escape puts the actions away and keeps focus on the row", async ({ page }) => {
    await ready(page, "#/memories/ent-leo", "Leo");
    const row = page.getByRole("button", { name: "Leo is allergic to nuts." });
    await row.click();
    await page.getByRole("button", { name: "Keep forever" }).focus();
    await page.keyboard.press("Escape");
    await expect(row).toHaveAttribute("aria-expanded", "false");
    await expect(row).toBeFocused();
  });

  test("the tabs follow the arrow keys", async ({ page }) => {
    await ready(page, "#/memories", "What Nocturne remembers");
    await page.getByRole("tab", { name: "People" }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("tab", { name: "Places" })).toBeFocused();
    await expect(page.getByRole("link", { name: /Rowan Court/ })).toBeVisible();
    await page.keyboard.press("End");
    await expect(page.getByRole("tab", { name: "Things" })).toBeFocused();
    await expect(page.getByRole("link", { name: /Dentist appointment/ })).toBeVisible();
  });
});

test.describe("using it", () => {
  test("searching fills the address and lists what matches", async ({ page }) => {
    await ready(page, "#/memories", "What Nocturne remembers");
    await page.getByRole("searchbox", { name: "Search everything kept" }).fill("dentist");
    await expect(page).toHaveURL(/#\/memories\?q=dentist$/);
    await expect(page.getByRole("heading", { name: /memories? match “dentist”/ })).toBeVisible();
  });

  test("a receipt opens the original, and a memory there opens its person", async ({ page }) => {
    await ready(page, "#/memories/ent-leo", "Leo");
    await page
      .getByRole("button", { name: /^Source: message you pasted/ })
      .first()
      .click();
    await expect(page).toHaveURL(/#\/items\/item-swim$/);
    await page.getByRole("link", { name: "Leo’s swimming is on Mondays at 17:30." }).click();
    await expect(page).toHaveURL(/#\/memories\/ent-leo$/);
  });

  test("forgetting a memory leaves focus on the person's name", async ({ page }) => {
    await ready(page, "#/memories/ent-leo", "Leo");
    await page.getByRole("button", { name: "Leo is allergic to nuts." }).click();
    await page.getByRole("button", { name: "Forget…" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("button", { name: "Keep them" })).toBeFocused();
    await dialog.getByRole("button", { name: /^Forget 2 things/ }).click();
    await expect(dialog.getByText("Gone.")).toBeVisible();
    await dialog.getByRole("button", { name: "OK" }).click();
    await expect(page.getByText("Leo is allergic to nuts.")).toHaveCount(0);
    await expect(page.getByRole("heading", { level: 1, name: "Leo" })).toBeFocused();
  });

  test("a recording has a play button and a position control", async ({ page }) => {
    await ready(page, "#/items/item-lake", "Lake District");
    await expect(page.getByRole("button", { name: "Play" })).toBeVisible();
    await expect(page.getByRole("slider", { name: "Position in the recording" })).toBeVisible();
    await expect(
      page.getByText("0:00 to 0:42: You decided to book the Lake District cottage for half term."),
    ).toBeVisible();
  });

  test("a photo shows the picture and the words beside it", async ({ page }) => {
    await ready(page, "#/items/item-letter", "Letter");
    await expect(page.getByRole("img", { name: "Photo you kept: Letter" })).toBeVisible();
    await expect(page.getByRole("region", { name: "Memories from it · 4" })).toBeVisible();
  });
});
