import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "../e2e/axe";

// The app's screens at 412 by 892, light and dark, each checked with axe.

async function clean(page: Page) {
  const result = await runAxe(page);
  expect(result.violations.map((v) => `${v.id}: ${v.nodes[0]?.target}`)).toEqual([]);
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme}`, () => {
    test.use({ colorScheme: scheme });

    test("the bar, and Today with last night and the calendar", async ({ page }) => {
      await page.goto("/");
      const nav = page.getByRole("navigation", { name: "Nocturne" });
      await expect(nav.getByRole("button", { name: "Today" })).toHaveAttribute(
        "aria-current",
        "page",
      );
      await expect(nav.getByRole("button", { name: "Journal" })).toBeVisible();
      await expect(nav.getByRole("button", { name: "Ask" })).toBeVisible();
      await expect(page.getByRole("button", { name: "Keep" })).toBeVisible();
      await expect(page.getByRole("button", { name: "This phone" })).toBeVisible();
      await expect(page.getByText("Connected to Home computer")).toBeVisible();
      await expect(page.getByRole("heading", { name: "Last night" })).toBeVisible();
      await expect(page.getByText("6 h 40 min asleep", { exact: true })).toBeVisible();
      await expect(page.getByText("11:50 pm to 7:05 am, awake once.")).toBeVisible();
      await expect(page.getByRole("button", { name: "Sleep · Health · Pixel" })).toHaveText(
        "Sleep · Health",
      );
      await expect(page.getByRole("heading", { name: "In your calendar today · 3" })).toBeVisible();
      await expect(
        page.getByRole("button", {
          name: "9:00 am, Dentist, check-up, at Bridge Street Surgery, from your Family calendar",
        }),
      ).toBeVisible();
      await clean(page);

      await nav.getByRole("button", { name: "Ask" }).click();
      await expect(nav.getByRole("button", { name: "Ask" })).toHaveAttribute(
        "aria-current",
        "page",
      );
      await expect(page.getByRole("button", { name: "Keep" })).toHaveCount(0);
      // The question field sits above the bar, and asking works.
      const field = page.getByRole("textbox");
      await field.click();
      await field.fill("When is the dentist?");
      await field.press("Enter");
      await expect(page.getByText("When is the dentist?").first()).toBeVisible();
      const box = await page.locator(".ph-askbar").boundingBox();
      const bar = await nav.boundingBox();
      expect(box && bar && box.y + box.height <= bar.y + 1).toBe(true);
    });

    test("the journal with the calendar and From Health", async ({ page }) => {
      await page.goto("/?v=journal");
      await expect(page.getByText("Woke · 6 h 40 min asleep")).toBeVisible();
      await expect(page.getByText("from your calendar")).toHaveCount(2);
      await expect(page.getByText("Walk · 42 min · 3.1 km")).toBeVisible();
      await expect(page.getByText("after 6 h 40 min asleep")).toBeVisible();
      const health = page.getByRole("region", { name: "From Health" });
      await expect(health).toBeVisible();
      await expect(health.getByText("8,412 today")).toBeVisible();
      await clean(page);
      // The last button on the day is reachable above the bar.
      const last = page.getByRole("button", { name: /^← / });
      await last.scrollIntoViewIfNeeded();
      await last.click({ trial: true });
    });

    test("the calendar refused from the offer", async ({ page }) => {
      await page.goto("/?n=calendarRefused&v=offer");
      await expect(
        page.getByRole("heading", { name: "Bring more from this phone?" }),
      ).toBeVisible();
      await clean(page);
      await page.getByRole("button", { name: /Your calendar/ }).click();
      await expect(
        page.getByRole("heading", { name: "Bring your calendar to your computer?" }),
      ).toBeVisible();
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(
        page.getByRole("heading", { name: "Your calendar stays on this phone." }),
      ).toBeVisible();
      await clean(page);
      await page.getByRole("button", { name: "OK" }).click();
      await expect(page.getByRole("button", { name: /Your calendar.*Not brought/ })).toBeVisible();
      await page.getByRole("button", { name: "Done" }).click();
      await expect(page.getByRole("navigation", { name: "Nocturne" })).toBeVisible();
    });

    test("Health Connect missing", async ({ page }) => {
      await page.goto("/?n=healthMissing&v=thisPhone");
      await page.getByRole("button", { name: /Health/ }).click();
      await expect(page.getByRole("heading", { name: "Health Connect is needed" })).toBeVisible();
      await clean(page);
    });

    test("This phone", async ({ page }) => {
      await page.goto("/");
      await page.getByRole("button", { name: "This phone" }).click();
      await expect(page.getByRole("region", { name: "Connected to" })).toContainText(
        "Simple, through Cloudflare",
      );
      const brings = page.getByRole("region", { name: "What this phone brings" });
      await expect(
        brings.getByRole("button", { name: /Your calendar.*On · 3 calendars/ }),
      ).toBeVisible();
      await expect(brings.getByRole("button", { name: /Health.*On · 6 kinds/ })).toBeVisible();
      await expect(brings).toContainText("4 calendar changes, 18 readings");
      await expect(page.getByText("Rename it on your computer")).toBeVisible();
      await clean(page);
      await page.getByRole("button", { name: "Disconnect this phone…" }).click();
      await expect(page.getByRole("heading", { name: "Disconnect this phone?" })).toBeVisible();
    });

    for (const [row, title, switchName] of [
      [/Your calendar/, "calendar", "Bring my calendar"],
      [/Health/, "health", "Bring health readings"],
    ] as const) {
      test(`${title} settings end with Done`, async ({ page }) => {
        await page.goto("/");
        await page.getByRole("button", { name: "This phone" }).click();
        await page
          .getByRole("region", { name: "What this phone brings" })
          .getByRole("button", { name: row })
          .click();
        await expect(page.getByRole("switch", { name: new RegExp(switchName) })).toBeVisible();
        await expect(page.getByRole("region", { name: "Kept so far" })).toHaveCount(0);
        const done = page.getByRole("button", { name: "Done" });
        await expect(done).toBeVisible();
        const last = await page.evaluate(() => {
          const buttons = [...document.querySelectorAll("button")];
          return buttons[buttons.length - 1].textContent;
        });
        expect(last).toBe("Done");
        await clean(page);
        await page.emulateMedia({ reducedMotion: "reduce" });
        await page.screenshot({
          path: `${process.env.SHOTS ?? "test-results/phone-native-shots"}/companion-${title}-${scheme}.png`,
        });
        await done.click();
        await expect(page.getByRole("region", { name: "What this phone brings" })).toBeVisible();
      });
    }

    test("keeping what another app shared", async ({ page }) => {
      await page.goto("/?n=share");
      const sheet = page.getByRole("dialog", { name: "Keep in Nocturne" });
      await expect(sheet).toBeVisible();
      await expect(sheet.getByText("https://example.org/school-trip-letter")).toBeVisible();
      await expect(sheet.getByLabel("Add a note (optional)")).toBeVisible();
      await clean(page);
      await sheet.getByLabel("Add a note (optional)").fill("Trip form, due back Monday");
      await sheet.getByRole("button", { name: "Keep" }).click();
      await expect(sheet.getByRole("heading", { name: "Kept." })).toBeVisible();
    });

    test("a shared photo shows its name and size", async ({ page }) => {
      await page.goto("/?n=shareFile");
      const sheet = page.getByRole("dialog", { name: "Keep in Nocturne" });
      await expect(sheet.getByText(/Photo · 2.4\sMB/)).toBeVisible();
      await expect(sheet.getByText("School letter.png")).toBeVisible();
    });

    test("the computer away", async ({ page }) => {
      await page.goto("/?s=asleep");
      await expect(
        page.getByText("Home computer is asleep, switched off or has a new address."),
      ).toBeVisible();
      await expect(
        page.getByText("Today’s page is back when your computer is. This phone keeps no copy."),
      ).toBeVisible();
      await expect(page.getByText(/2 waiting/)).toBeVisible();
      await expect(page.getByRole("button", { name: "Keep" })).toBeVisible();
      await clean(page);
    });

    test("update this phone", async ({ page }) => {
      await page.goto("/?s=outdated");
      await expect(
        page.getByRole("heading", { name: "Update Nocturne on this phone" }),
      ).toBeVisible();
      await expect(
        page.getByText(
          "Your computer has a newer Nocturne that this version can’t talk to. What you keep waits here until you update.",
        ),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Update in Google Play" })).toBeVisible();
      await expect(
        page.getByText(/^Your computer: Nocturne 0\.9\.0 · This phone: Nocturne /),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Keep" })).toBeVisible();
      await clean(page);
    });

    test("update the computer", async ({ page }) => {
      await page.goto("/?s=computerOutdated");
      await expect(
        page.getByRole("heading", { name: "Update Nocturne on your computer" }),
      ).toBeVisible();
      await expect(
        page.getByText(
          "This phone has a newer Nocturne than your computer. What you keep waits here until your computer is updated.",
        ),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Update in Google Play" })).toHaveCount(0);
      await expect(
        page.getByText(/^Your computer: Nocturne 0\.1\.0 · This phone: Nocturne /),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Keep" })).toBeVisible();
      await clean(page);
    });
  });
}
