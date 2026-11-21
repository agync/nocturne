import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

// From your phones, what came in, the stop and delete dialogs, and the Away from home changes,
// with Sam's phone (the app) and Ana's phone (the browser) paired. 1280 wide, light and dark, axe.

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const clean = async (page: Page, name: string) => {
  const results = await runAxe(page, (b) => b.withTags(TAGS));
  expect(results.violations.map((v) => `${name} ${v.id}: ${v.nodes[0]?.html}`)).toEqual([]);
};

for (const scheme of ["light", "dark"] as const) {
  test.describe(`from your phones, ${scheme}`, () => {
    test.use({ colorScheme: scheme, viewport: { width: 1280, height: 900 } });

    const ready = async (page: Page, hash: string, away = false) => {
      await page.goto(`/?mockPhones=1${away ? "&mockAway=on" : ""}${hash}`);
      await page.getByRole("heading", { level: 1 }).first().waitFor();
      await page.evaluate(() => document.fonts.ready);
    };

    test("Settings offers the page, and the page says what each phone brings", async ({ page }) => {
      await ready(page, "#/settings");
      await page.getByRole("link", { name: /From your phones/ }).click();
      await expect(page.getByRole("heading", { level: 1, name: "From your phones" })).toBeVisible();
      await expect(page.getByText("Android app · Nocturne 1.2")).toBeVisible();
      await expect(page.getByText(/3 calendars · last came in .*7:42 am/)).toBeVisible();
      await expect(page.getByText("In the browser").first()).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-sources-${scheme}.png` });
      await clean(page, "sources");
    });

    test("What came in lists the changes newest first, each opening its page", async ({ page }) => {
      await ready(page, "#/settings/phones");
      await page.getByRole("link", { name: /See what came in/ }).click();
      await expect(
        page.getByRole("heading", { level: 1, name: "Calendar from Sam’s phone" }),
      ).toBeVisible();
      await expect(page.getByText("removed from your calendar")).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-came-in-${scheme}.png` });
      await clean(page, "came in");
      await page.getByRole("link", { name: /Book group/ }).click();
      await expect(page.getByRole("heading", { level: 1, name: "Book group" })).toBeVisible();
    });

    test("Stop taking it asks first, and says Stopped after", async ({ page }) => {
      await ready(page, "#/settings/phones");
      await page.getByRole("button", { name: "Stop taking the calendar from Sam’s phone" }).click();
      const dialog = page.getByRole("dialog", {
        name: "Stop taking the calendar from Sam’s phone?",
      });
      await page.screenshot({ path: `${SHOTS}/phones-stop-${scheme}.png` });
      await clean(page, "stop");
      await dialog.getByRole("button", { name: "Stop" }).click();
      await expect(page.getByText("Stopped").first()).toBeVisible();
      await expect(
        page.getByRole("button", { name: /Take the calendar from Sam’s phone again/ }).first(),
      ).toBeVisible();
    });

    test("Delete what came in previews, offers to stop, and deletes", async ({ page }) => {
      await ready(page, "#/settings/phones");
      await page.getByRole("button", { name: /Delete what came in/ }).click();
      const dialog = page.getByRole("dialog");
      await expect(
        dialog.getByText("Your phone’s calendar isn’t touched.", { exact: false }),
      ).toBeVisible();
      await expect(
        dialog.getByRole("checkbox", { name: /Also stop taking the calendar/ }),
      ).toBeChecked();
      await page.screenshot({ path: `${SHOTS}/phones-delete-${scheme}.png` });
      await clean(page, "delete");
      await dialog.getByRole("button", { name: "Delete", exact: true }).click();
      await expect(dialog).toBeHidden();
    });

    test("Add a phone tells the app from the browser", async ({ page }) => {
      await ready(page, "#/settings/away/add", true);
      await expect(
        page.getByText("Scan it with the Nocturne app on your phone.", { exact: false }),
      ).toBeVisible();
      await expect(page.getByText("The four words")).toBeVisible();
      await expect(page.getByText(/No app\? In your phone’s browser, open/)).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/phones-add-${scheme}.png` });
      await clean(page, "add a phone");
    });

    test("Removing a phone offers Also delete what it brought, unticked", async ({ page }) => {
      await ready(page, "#/settings/away/phones", true);
      await expect(page.getByText(/Android app · Nocturne 1.2/)).toBeVisible();
      await page.getByRole("button", { name: "Remove Sam’s phone" }).click();
      const dialog = page.getByRole("dialog", { name: "Remove Sam’s phone?" });
      await expect(
        dialog.getByRole("checkbox", { name: "Also delete what it brought" }),
      ).not.toBeChecked();
      await page.screenshot({ path: `${SHOTS}/phones-remove-${scheme}.png` });
      await clean(page, "remove");
    });

    test("Privacy says phones can bring their calendar and health", async ({ page }) => {
      await ready(page, "#/privacy", true);
      await expect(
        page.getByText(
          "Your phones can bring their calendar and health readings, in the same locked messages.",
        ),
      ).toBeVisible();
      await clean(page, "privacy");
    });
  });
}
