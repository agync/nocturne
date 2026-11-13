import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";

const ready = async (page: Page, hash = "#/today") => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

test.describe("screenshots of #/today", () => {
  test("light", async ({ page }) => {
    await ready(page);
    await page.screenshot({ path: `${SHOTS}/today-light.png` });
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
    await page.screenshot({ path: `${SHOTS}/today-dark.png` });
    await context.close();
  });

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("400 wide", async ({ page }) => {
      await ready(page);
      await page.screenshot({ path: `${SHOTS}/today-narrow.png` });
    });
  });

  test("200% text", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
    await page.waitForTimeout(200);
    await page.screenshot({ path: `${SHOTS}/today-text-200.png` });
  });
});

test.describe("the page works", () => {
  test("choosing a dentist time resolves it and announces it politely", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Two things need you today.");
    await page.getByRole("button", { name: /Thursday 19 Nov, 10:00/ }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("One thing needs you today.");
    const status = page.getByRole("status").filter({ hasText: "set to" });
    await expect(status).toHaveText("Dentist set to Thursday 19 Nov, 10:00.");
    await expect(status).toHaveAttribute("aria-live", "polite");
    await expect(
      page.getByText(/it is\. The other is kept as history, not deleted\./),
    ).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: /Needs you/ }),
    ).toContainText("1");
  });

  test("marking the form done strikes it through", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Mark as done" }).click();
    const line = page.getByText(/The Year 4 trip form is due/);
    await expect(line).toHaveCSS("text-decoration-line", "line-through");
    await expect(page.getByText("Done. Nocturne won’t bring it up again.")).toBeVisible();
  });

  test("a let-go line comes back", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Bring back" }).first().click();
    await expect(page.getByText("Brought back. It will not fade for 30 days.")).toBeVisible();
  });

  test("answering the question says thanks", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Yes" }).click();
    await expect(page.getByText("Kept as sure.")).toBeVisible();
    await page.getByRole("button", { name: "Change answer" }).click();
    await expect(page.getByRole("button", { name: "Yes" })).toBeVisible();
  });

  test("keeping something from the bar says so and adds it to the list", async ({ page }) => {
    await ready(page);
    await page.getByRole("textbox", { name: "Keep something, or ask" }).fill("Buy milk");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept.", { exact: true })).toBeVisible();
    await expect(
      page.getByRole("complementary", { name: "Recently kept" }).getByText("Buy milk"),
    ).toBeVisible();
  });

  test("the whole page can be used from the keyboard", async ({ page }) => {
    await ready(page);
    await page.getByRole("main").focus();
    const reached: string[] = [];
    for (let i = 0; i < 30; i += 1) {
      await page.keyboard.press("Tab");
      reached.push(
        await page.evaluate(() => (document.activeElement?.textContent ?? "").trim().slice(0, 30)),
      );
    }
    for (const word of [
      "Read",
      "Mark as done",
      "Tuesday 17 Nov",
      "Thursday 19 Nov",
      "Bring back",
      "Yes",
    ]) {
      expect(
        reached.some((r) => r.includes(word)),
        word,
      ).toBe(true);
    }
  });

  test("the receipt of a claim opens its original", async ({ page }) => {
    await ready(page);
    await page
      .getByRole("button", { name: /^Letter/ })
      .first()
      .click();
    await expect(page).toHaveURL(/#\/items\/item-letter$/);
  });
});

const VARIANTS: Array<[string, string, string]> = [
  ["first", "#/pages/2026-10-31", "Your first page."],
  ["heavy", "#/pages/2026-11-04", "Seven things need you. Three are today."],
  ["quiet", "#/pages/2026-11-05", "A quiet night. Nothing needs you."],
  ["being written", "#/pages/2026-11-06", "Your page is being written."],
  ["catch-up", "#/pages/2026-11-07", "Two nights in one page."],
];

test.describe("other days", () => {
  for (const [name, hash, headline] of VARIANTS) {
    test(`${name} page renders`, async ({ page }) => {
      await ready(page, hash);
      await expect(page.getByRole("heading", { level: 1 })).toHaveText(headline);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      ).toBeLessThanOrEqual(0);
      await page.screenshot({
        path: `${SHOTS}/page-${name.replace(" ", "-")}.png`,
        fullPage: false,
      });
    });
  }
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on every day in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    for (const hash of ["#/today", ...VARIANTS.map((v) => v[1])]) {
      await ready(page, hash);
      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
        .analyze();
      expect(
        results.violations.map(
          (v) => `${hash} ${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
        ),
      ).toEqual([]);
    }
    await context.close();
  });
}

test("axe passes on today after the page has been used", async ({ page }) => {
  await ready(page);
  await page.getByRole("button", { name: "Mark as done" }).click();
  await page.getByRole("button", { name: /Thursday 19 Nov/ }).click();
  await page.getByRole("button", { name: "Yes" }).click();
  await page.getByRole("button", { name: "Bring back" }).first().click();
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
    .analyze();
  expect(results.violations.map((v) => v.id)).toEqual([]);
});

test.describe("focus stays with the person after an action", () => {
  const stays = async (page: Page, container: string) => {
    const inside = await page.evaluate((selector) => {
      const active = document.activeElement;
      return Boolean(active && active !== document.body && active.closest(selector));
    }, container);
    expect(inside).toBe(true);
  };
  const press = async (page: Page, name: string | RegExp) => {
    const button = page.getByRole("button", { name }).first();
    await button.focus();
    await page.keyboard.press("Enter");
  };

  test("Mark as done", async ({ page }) => {
    await ready(page);
    await press(page, "Mark as done");
    await stays(page, '[data-kind="due"]');
  });

  test("choosing which is right", async ({ page }) => {
    await ready(page);
    await press(page, /Thursday 19 Nov/);
    await stays(page, '[data-kind="which"]');
  });

  test("Bring back", async ({ page }) => {
    await ready(page);
    await press(page, "Bring back");
    await stays(page, '[data-kind="letGo"]');
  });

  test("answering the question", async ({ page }) => {
    await ready(page);
    await press(page, "Yes");
    await stays(page, "section");
    await expect(page.getByRole("button", { name: "Change answer" })).toBeFocused();
  });
});

test.describe("a screen that fails does not take the window with it", () => {
  test("a page that is not a date says so and the shell stays", async ({ page }) => {
    await ready(page, "#/pages/not-a-date");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("That page isn\u2019t here");
    await expect(page.getByRole("navigation", { name: "Main" })).toBeVisible();
  });
});

test.describe("which-is-right cards on a narrow window", () => {
  test.use({ viewport: { width: 320, height: 700 } });
  test("stack instead of overflowing", async ({ page }) => {
    await ready(page);
    const cards = page.getByRole("button", { name: /Tuesday 17 Nov|Thursday 19 Nov/ });
    const [a, b] = [await cards.nth(0).boundingBox(), await cards.nth(1).boundingBox()];
    expect((b?.y ?? 0) > (a?.y ?? 0) + (a?.height ?? 0) - 1).toBe(true);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      ),
    ).toBeLessThanOrEqual(0);
  });
});
