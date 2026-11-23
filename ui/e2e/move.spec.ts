import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// A phone's things coming to this computer, on Away from home: each state light and dark, read
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

const CHOOSE = "Maya’s phone has its own memory, and this computer has one too.";

const states: Array<[string, string | RegExp]> = [
  ["here", CHOOSE],
  ["room", CHOOSE],
  ["preparing", "Maya’s phone is getting its things ready…"],
  ["arriving", /Bringing in Maya’s phone’s things/],
  ["adding", "Adding them to the memory here…"],
  ["brought", "Maya’s phone’s things are here."],
  ["used", "Maya’s phone now works with this computer."],
  ["ranOut", /There wasn’t room for Maya’s phone’s memory/],
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
  await expect(dialog.getByRole("radio", { name: /Bring the phone’s things in/ })).toBeChecked();
  await expect(dialog.getByRole("button", { name: "Continue" })).toBeInViewport();
  const wide = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(wide).toBeLessThanOrEqual(1);
  await page.close();
});

test("Continue goes on to the things arriving, and Open today’s page to today", async ({
  page,
}) => {
  await open(page, "here");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await page.goto("/?mockAway=on&mockMove=brought#/settings/away");
  await page.getByRole("button", { name: "Open today’s page" }).click();
  await expect(page).toHaveURL(/#\/today/);
});
