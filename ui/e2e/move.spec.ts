import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// A phone's memory moving to this computer, on Away from home: each state light and dark, read
// by axe, and whole at a phone's width.

const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const open = async (page: Page, state: string) => {
  await page.goto(`/?mockAway=on&mockMove=${state}#/settings/away`);
  await page.getByRole("alertdialog").waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) =>
  (await runAxe(page, (b) => b.withTags(TAGS))).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const states: Array<[string, string | RegExp]> = [
  ["offered", "Maya’s phone has its own memory. Move it here?"],
  ["here", "Maya’s phone has its own memory. Move it here?"],
  ["arriving", "Maya’s phone has its own memory. Move it here?"],
  ["checking", "Maya’s phone has its own memory. Move it here?"],
  ["arrived", "The memory from Maya’s phone is here."],
];

for (const scheme of ["light", "dark"] as const) {
  test.describe(scheme, () => {
    test.use({ colorScheme: scheme, viewport: { width: 1280, height: 800 } });
    for (const [state, title] of states) {
      test(`${state} reads clean`, async ({ page }) => {
        await open(page, state);
        await expect(page.getByRole("alertdialog", { name: title })).toBeVisible();
        expect(await axe(page)).toEqual([]);
        await page.screenshot({ path: `test-results/move/move-${state}-${scheme}.png` });
      });
    }
  });
}

test("the choice with a memory here, and its words, at a phone's width", async ({ browser }) => {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  await open(page, "here");
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("This computer already holds a memory.");
  await expect(dialog.getByRole("button", { name: "Keep what’s here" })).toBeVisible();
  await expect(
    dialog.getByRole("button", { name: "Put this aside and move it here" }),
  ).toBeInViewport();
  const wide = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(wide).toBeLessThanOrEqual(1);
  await page.close();
});

test("Move it here goes on to arriving, and Open today’s page to today", async ({ page }) => {
  await open(page, "offered");
  await page.getByRole("button", { name: "Move it here" }).click();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await page.goto("/?mockAway=on&mockMove=arrived#/settings/away");
  await page.getByRole("button", { name: "Open today’s page" }).click();
  await expect(page).toHaveURL(/#\/today/);
});

test("after the start that put it in place, it says so over today's page", async ({ page }) => {
  await page.goto("/?mockMove=arrived#/today");
  await expect(
    page.getByRole("alertdialog", { name: "The memory from Maya’s phone is here." }),
  ).toBeVisible();
  expect(await axe(page)).toEqual([]);
});
