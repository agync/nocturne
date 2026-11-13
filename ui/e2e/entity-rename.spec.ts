import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const open = async (page: Page, hash = "#/memories/ent-leo", heading = "Leo") => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1, name: heading }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const violations = async (page: Page, where: string) => {
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  return results.violations.map(
    (v) => `${where} ${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );
};

test("renames a person, shows the old name, and goes back", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Rename" }).click();
  const dialog = page.getByRole("dialog", { name: "Rename" });
  await expect(dialog.getByRole("textbox", { name: "What is it called?" })).toBeFocused();
  await dialog.getByRole("textbox", { name: "What is it called?" }).fill("Leopold");
  await dialog.getByRole("button", { name: "Rename" }).click();
  await expect(page.getByText("Renamed to Leopold.")).toBeVisible();
  await expect(page.getByRole("heading", { level: 1, name: "Leopold" })).toBeFocused();
  await expect(page.getByText("Also called Leo.")).toBeVisible();
  await expect(page.getByText("Leopold’s swimming is on Mondays at 17:30.")).toBeVisible();
  await expect(page.getByRole("link", { name: /^Leopold/ })).toBeVisible();

  await page.getByRole("button", { name: "Go back to Leo" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Leo" })).toBeVisible();
  await expect(page.getByText("Also called")).toHaveCount(0);
});

test("a name something already has is offered as a merge, with a way back", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Rename" }).click();
  await page.getByRole("textbox", { name: "What is it called?" }).fill("Ava");
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Is this the same as Ava?" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Merge them" }).click();
  await expect(page.getByText("Merged into Ava.")).toBeVisible();
  await expect(page).toHaveURL(/#\/memories\/ent-ava$/);
  await expect(page.getByText("Also called Leo.")).toBeVisible();
  await page.getByRole("button", { name: "Undo the merge with Leo" }).click();
  await page.goto("/#/memories/ent-leo");
  await expect(page.getByRole("heading", { level: 1, name: "Leo" })).toBeVisible();
});

test("moves one memory to something else from its actions, and undoes it", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: /^Leo needs a packed lunch/ }).click();
  await page.getByRole("button", { name: "Is this about something else?" }).click();
  const dialog = page.getByRole("dialog", { name: "Is this about something else?" });
  await dialog.getByRole("textbox", { name: "Find a person, place or thing" }).fill("okaf");
  await dialog.getByRole("radio", { name: /^Mrs Okafor/ }).check();
  await dialog.getByRole("button", { name: "Move it" }).click();
  await expect(page.getByText("Moved to Mrs Okafor.")).toBeVisible();
  await expect(page.getByText("Leo needs a packed lunch on the trip day.")).toHaveCount(0);
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(page.getByRole("button", { name: /^Leo needs a packed lunch/ })).toBeVisible();
});

test("offers to move a memory right after it is corrected", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: /^Leo is allergic to nuts\./ }).click();
  await page.getByRole("button", { name: "Correct" }).click();
  await page.getByRole("textbox", { name: "What should it say?" }).fill("Ava is allergic to nuts.");
  await page.keyboard.press("Enter");
  await expect(page.getByText("Corrected.")).toBeVisible();
  await page
    .locator('[data-region="toast"]')
    .getByRole("button", { name: "Is this about something else?" })
    .click();
  const dialog = page.getByRole("dialog", { name: "Is this about something else?" });
  await expect(dialog.getByText("Ava is allergic to nuts.")).toBeVisible();
  await dialog.getByRole("radio", { name: "Someone or something new" }).check();
  await dialog.getByRole("textbox", { name: "What is it called?" }).fill("Nut allergies");
  await dialog.getByRole("button", { name: "Move it" }).click();
  await expect(page.getByText("Moved to Nut allergies.")).toBeVisible();
});

test("the dialogs fit a narrow window", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 760 });
  await open(page);
  await page.getByRole("button", { name: "Rename" }).click();
  await expect(page.getByRole("dialog", { name: "Rename" })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on the rename and move dialogs in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    await open(page);
    const found: string[] = [];

    await found.push(...(await violations(page, "entity")));

    await page.getByRole("button", { name: "Rename" }).click();
    await page.getByRole("dialog", { name: "Rename" }).waitFor();
    found.push(...(await violations(page, "rename")));
    await page.getByRole("textbox", { name: "What is it called?" }).fill("Ava");
    await page.keyboard.press("Enter");
    await page.getByRole("dialog", { name: "Is this the same as Ava?" }).waitFor();
    found.push(...(await violations(page, "merge")));
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: /^Leo needs a packed lunch/ }).click();
    await page.getByRole("button", { name: "Is this about something else?" }).click();
    const dialog = page.getByRole("dialog", { name: "Is this about something else?" });
    await dialog.getByRole("radio", { name: /^Ava/ }).waitFor();
    found.push(...(await violations(page, "move existing")));
    await dialog.getByRole("radio", { name: /^Ava/ }).check();
    found.push(...(await violations(page, "move chosen")));
    await dialog.getByRole("radio", { name: "Someone or something new" }).check();
    found.push(...(await violations(page, "move new")));
    if (scheme === "dark") await page.screenshot({ path: "test-results/screens/refile-dark.png" });

    expect(found).toEqual([]);
    await context.close();
  });
}
