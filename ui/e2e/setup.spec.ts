import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

/** The mock engine starts in the named state when the address says so: `/?mockSetup=<name>`. */
const SCREENS: Array<[string, string, string, string | RegExp]> = [
  ["setup-welcome", "notStarted", "#/setup", /Keep anything/],
  ["setup-checking", "checking", "#/setup?step=choosing", "Checking this computer…"],
  ["setup-tiers", "choosing", "#/setup?step=choosing", /lighter mind/],
  ["setup-unsupported", "unsupported", "#/setup?step=choosing", /enough computer memory/],
  [
    "setup-downloading",
    "downloading",
    "#/setup",
    "While Nocturne gets ready, keep your first thing.",
  ],
  ["setup-offline", "offline", "#/setup", "While Nocturne gets ready, keep your first thing."],
  ["setup-disk-full", "diskFull", "#/setup", "While Nocturne gets ready, keep your first thing."],
  ["setup-done", "done", "#/setup", "Nocturne is ready."],
];

const ready = async (page: Page, scenario: string, hash: string, heading: string | RegExp) => {
  await page.goto(`/?mockSetup=${scenario}${hash}`);
  await page.getByRole("heading", { level: 1, name: heading }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("screenshots", () => {
  test.use({ viewport: { width: 1100, height: 760 } });
  for (const [name, scenario, hash, heading] of SCREENS) {
    test(`${name} light`, async ({ page }) => {
      await ready(page, scenario, hash, heading);
      await page.screenshot({ path: `${SHOTS}/${name}-light.png` });
    });

    test(`${name} dark`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: "dark",
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1100, height: 760 },
      });
      const page = await context.newPage();
      await ready(page, scenario, hash, heading);
      await page.screenshot({ path: `${SHOTS}/${name}-dark.png` });
      await context.close();
    });
  }
});

test.describe("at the smallest window", () => {
  test.use({ viewport: { width: 720, height: 640 } });
  for (const [name, scenario, hash, heading] of SCREENS) {
    test(`${name} fits 720 px`, async ({ page }) => {
      await ready(page, scenario, hash, heading);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
    });
  }
});

test.describe("at 200% zoom of the smallest window (WCAG 1.4.10)", () => {
  test.use({ viewport: { width: 360, height: 640 } });
  for (const [name, scenario, hash, heading] of SCREENS) {
    test(`${name} does not scroll sideways`, async ({ page }) => {
      await ready(page, scenario, hash, heading);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      // Nothing is squeezed past its own box and cut off by the card.
      const cut = await page.evaluate(() =>
        [...document.querySelectorAll<HTMLElement>("main *")]
          .filter((el) => el.clientWidth > 0 && el.scrollWidth > el.clientWidth + 1)
          .filter((el) => getComputedStyle(el).overflowX === "visible")
          .map((el) => `${el.tagName}.${el.className}: ${el.textContent?.slice(0, 24)}`),
      );
      expect(cut).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/${name}-360.png`, fullPage: true });
    });
  }
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on every state of setup in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1100, height: 760 },
    });
    const page = await context.newPage();
    for (const [name, scenario, hash, heading] of SCREENS) {
      await ready(page, scenario, hash, heading);
      const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
      expect(
        results.violations.map(
          (v) => `${name} ${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
        ),
      ).toEqual([]);
    }
    await context.close();
  });
}

test.describe("using it", () => {
  test.use({ viewport: { width: 1100, height: 760 } });

  test("the welcome is always at night, and Get started leaves it", async ({ page }) => {
    await ready(page, "notStarted", "#/setup", /Keep anything/);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await page.getByRole("button", { name: "Get started" }).click();
    await expect(
      page.getByRole("heading", { level: 1, name: "Checking this computer…" }),
    ).toBeVisible();
  });

  test("choosing a tier changes the button, and setting up moves to getting ready", async ({
    page,
  }) => {
    await ready(page, "choosing", "#/setup?step=choosing", /lighter mind/);
    await page.getByText("Why this one?").click();
    await expect(page.getByText(/Large needs 60 GB or more/)).toBeVisible();
    await page.getByRole("radio", { name: /Small/ }).check({ force: true });
    await expect(page.getByRole("button", { name: "Set up · 4.8 GB" })).toBeVisible();
    await page.getByRole("radio", { name: /Standard/ }).check({ force: true });
    await page.getByRole("button", { name: "Set up · 8.4 GB" }).click();
    await expect(
      page.getByRole("heading", {
        level: 1,
        name: "While Nocturne gets ready, keep your first thing.",
      }),
    ).toBeVisible();
  });

  test("the tiers are one radio group and the arrow keys move between them", async ({ page }) => {
    await ready(page, "large", "#/setup?step=choosing", /best mind/);
    await page.getByRole("radio", { name: /Large/ }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("radio", { name: /Standard/ })).toBeChecked();
  });

  test("pause and resume change the words and keep focus on the button", async ({ page }) => {
    await ready(
      page,
      "downloading",
      "#/setup",
      "While Nocturne gets ready, keep your first thing.",
    );
    const pause = page.getByRole("button", { name: "Pause" });
    await pause.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Resume" })).toBeFocused();
    await expect(page.getByText("Paused. Nothing is lost.").first()).toBeVisible();
  });

  test("a note can be kept while the mind downloads", async ({ page }) => {
    await ready(
      page,
      "downloading",
      "#/setup",
      "While Nocturne gets ready, keep your first thing.",
    );
    await page.getByRole("button", { name: "Type a note" }).click();
    await expect(page.getByRole("textbox", { name: "Your note" })).toBeFocused();
    await page.keyboard.type("Ask Sam about Saturday");
    await page.getByRole("button", { name: "Keep it" }).click();
    await expect(page.getByText("Kept. Nocturne will read it tonight.")).toBeVisible();
  });

  test("good defaults can be changed in place", async ({ page }) => {
    await ready(
      page,
      "downloading",
      "#/setup",
      "While Nocturne gets ready, keep your first thing.",
    );
    await page.getByRole("button", { name: "Change page time" }).click();
    const select = page.getByRole("combobox", { name: "Your page arrives at, time" });
    await expect(select).toBeFocused();
    await select.selectOption("06:30");
    await select.blur();
    await expect(page.getByText("06:30")).toBeVisible();
    await expect(page.getByRole("switch", { name: /update/i })).toHaveCount(0);
  });
});
