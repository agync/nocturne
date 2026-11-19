import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

/** The mock computer starts in the named state, and its checks end after `check` milliseconds. */
const open = async (page: Page, path: string, scenario = "off", check = 150) => {
  await page.goto(`/?mockOnline=${scenario}&mockCheckMs=${check}#${path}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) =>
  (await runAxe(page, (b) => b.withTags(TAGS))).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

/** Presses a control the way the keyboard does: focus it, then Enter or Space. */
const press = async (page: Page, role: "button" | "link" | "switch", name: string | RegExp) => {
  const target = page.getByRole(role, { name, exact: typeof name === "string" });
  await target.focus();
  await expect(target).toBeFocused();
  await page.keyboard.press(role === "switch" ? "Space" : "Enter");
};

/** Types into a field with the keyboard, after focusing it. */
const typeInto = async (page: Page, label: RegExp, text: string) => {
  const field = page.getByLabel(label);
  await field.focus();
  await expect(field).toBeFocused();
  await page.keyboard.type(text);
};

const connectAndCheck = async (page: Page) => {
  await press(page, "button", "Open Advanced details");
  await expect(
    page.getByRole("heading", { level: 1, name: "Connect to the service you chose." }),
  ).toBeVisible();
  await typeInto(page, /^Address/, "https://api.example.com/v1");
  await typeInto(page, /^Key/, "sk-example-not-a-real-key");
  await typeInto(page, /^Which one to use/, "llama-3.3-70b-instruct");
  await press(page, "button", "Check");
  await expect(page.getByText(/It works\. api\.example\.com answered/)).toBeVisible();
};

test.describe("O1 and O8, the page, in each state", () => {
  const states: Array<[string, string, string | RegExp]> = [
    ["off", "off", /It’s off\. Nothing is sent, and no service is connected/],
    ["off-before", "offBefore", "The service you chose is still set up"],
    ["on", "on", "38"],
    ["cost", "cost", "$0.42"],
    ["limit", "limit", "Paused by your limit"],
    ["refused", "refused", "Your provider refused the key"],
    ["no-credit", "noCredit", "Your provider says there’s no credit left"],
    ["problem", "problem", "Online answers aren’t working right now"],
  ];
  for (const [name, scenario, text] of states) {
    test(`${name}: axe clean in light and dark, no sideways scroll`, async ({ browser }) => {
      for (const scheme of ["light", "dark"] as const) {
        const context = await browser.newContext({
          colorScheme: scheme,
          locale: "en-GB",
          timezoneId: "Europe/London",
          viewport: { width: 1280, height: 800 },
        });
        const page = await context.newPage();
        await open(page, "/settings/online", scenario);
        await expect(page.getByText(text, { exact: false }).first()).toBeVisible();
        expect(await axe(page), `${name} ${scheme}`).toEqual([]);
        expect(await overflow(page)).toBeLessThanOrEqual(0);
        await page.screenshot({ path: `${SHOTS}/online-${name}-${scheme}.png` });
        await context.close();
      }
    });
  }

  test("narrow: nothing scrolls sideways at 400 wide", async ({ browser }) => {
    const context = await browser.newContext({ viewport: { width: 400, height: 760 } });
    const page = await context.newPage();
    await open(page, "/settings/online", "on");
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await context.close();
  });
});

test.describe("O13, turning off and forgetting", () => {
  test("keyboard only: switch off with Undo, then forget with a confirm", async ({ page }) => {
    await open(page, "/settings/online", "on");
    await press(page, "switch", "Online answers are on");
    await expect(
      page.getByText(/Online answers are off\. Questions will be answered/),
    ).toBeVisible();
    await press(page, "button", "Undo");
    await expect(page.getByRole("switch", { name: "Online answers are on" })).toBeChecked();
    await press(page, "button", "Turn off and forget the key…");
    const dialog = page.getByRole("dialog", { name: "Turn off and forget the key?" });
    await expect(dialog).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Turn off and forget");
    await expect(
      page.getByText("Turned off, and the key is forgotten on this computer."),
    ).toBeVisible();
    await expect(
      page.getByText(/It’s off\. Nothing is sent, and no service is connected/),
    ).toBeVisible();
  });
});

test.describe("O2 to O6, setting up", () => {
  const steps: Array<[string, (page: Page) => Promise<void>]> = [
    ["O2", async () => undefined],
    [
      "O4-service",
      async (page) => {
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 2 of 5")).toBeVisible();
        await expect(page.getByLabel(/^Address/)).toHaveCount(0);
      },
    ],
    [
      "O4-advanced-empty",
      async (page) => {
        await press(page, "button", "Continue");
        await press(page, "button", "Open Advanced details");
        await expect(page.getByLabel(/^Address/)).toBeVisible();
        await expect(page.getByRole("button", { name: "Continue" })).toBeDisabled();
      },
    ],
    [
      "O4-works",
      async (page) => {
        await press(page, "button", "Continue");
        await connectAndCheck(page);
      },
    ],
    [
      "O5",
      async (page) => {
        await press(page, "button", "Continue");
        await connectAndCheck(page);
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 3 of 5")).toBeVisible();
      },
    ],
    [
      "O7",
      async (page) => {
        await press(page, "button", "Continue");
        await connectAndCheck(page);
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 3 of 5")).toBeVisible();
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 4 of 5")).toBeVisible();
      },
    ],
    [
      "O6",
      async (page) => {
        await press(page, "button", "Continue");
        await connectAndCheck(page);
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 3 of 5")).toBeVisible();
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 4 of 5")).toBeVisible();
        await press(page, "button", "Continue");
        await expect(page.getByText("Online answers · step 5 of 5")).toBeVisible();
      },
    ],
  ];
  for (const [name, walk] of steps) {
    test(`${name}: axe clean in light and dark`, async ({ browser }) => {
      for (const scheme of ["light", "dark"] as const) {
        const context = await browser.newContext({
          colorScheme: scheme,
          locale: "en-GB",
          timezoneId: "Europe/London",
          viewport: { width: 1280, height: 800 },
        });
        const page = await context.newPage();
        await open(page, "/settings/online/setup");
        await walk(page);
        expect(await axe(page), `${name} ${scheme}`).toEqual([]);
        expect(await overflow(page)).toBeLessThanOrEqual(0);
        await page.screenshot({
          path: `${SHOTS}/online-setup-${name}-${scheme}.png`,
          fullPage: true,
        });
        await context.close();
      }
    });
  }

  test("a failed check is axe clean and keeps Continue off", async ({ page }) => {
    await open(page, "/settings/online/setup");
    await press(page, "button", "Continue");
    await press(page, "button", "Open Advanced details");
    await typeInto(page, /^Address/, "https://api.example.com/v1");
    await typeInto(page, /^Key/, "sk-refused");
    await typeInto(page, /^Which one to use/, "llama-3.3-70b-instruct");
    await press(page, "button", "Check");
    await expect(page.getByText("api.example.com refused the key.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(await axe(page)).toEqual([]);
  });

  test("keyboard only: the whole way to turned on", async ({ page }) => {
    await open(page, "/settings/online/setup");
    await press(page, "button", "Everything that is sent, and what stays here");
    await expect(page.getByText("when is my eye test?")).toBeVisible();
    await press(page, "button", "Continue");
    await connectAndCheck(page);
    await press(page, "button", "Continue");
    await press(page, "switch", "Health, money and ID details");
    await expect(page.getByRole("switch", { name: "Health, money and ID details" })).toBeChecked();
    await press(page, "button", "Continue");
    await press(page, "button", "Continue");
    await press(page, "button", "Turn on online answers");
    await expect(page.getByRole("switch", { name: "Online answers are on" })).toBeChecked();
    await expect(page.getByText("Health, money and ID details", { exact: true })).toBeVisible();
  });

  test("Tab alone: through the form, the check and on to Continue", async ({ page }) => {
    await open(page, "/settings/online/setup");
    await press(page, "button", "Continue");
    await press(page, "button", "Open Advanced details");
    const address = page.getByLabel(/^Address/);
    await address.focus();
    await page.keyboard.type("https://api.example.com/v1");
    await page.keyboard.press("Tab");
    await expect(page.getByLabel(/^Key/)).toBeFocused();
    await page.keyboard.type("sk-example-not-a-real-key");
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Show" })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(page.getByLabel(/^Which one to use/)).toBeFocused();
    await page.keyboard.type("llama-3.3-70b-instruct");
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Check" })).toBeFocused();
    await page.keyboard.press("Enter");
    const result = page.locator('[role=status][tabindex="-1"]');
    await expect(page.getByText(/It works\. api\.example\.com answered/)).toBeVisible();
    await expect(result).toBeFocused();
    await expect(page.getByRole("button", { name: "Check again" })).toBeDisabled();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "Continue" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByText("Online answers · step 3 of 5")).toBeVisible();
  });

  test("Advanced details: reached from Advanced details, in light and dark", async ({
    browser,
  }) => {
    for (const scheme of ["light", "dark"] as const) {
      const context = await browser.newContext({
        colorScheme: scheme,
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1280, height: 800 },
      });
      const page = await context.newPage();
      await open(page, "/settings/advanced");
      await press(page, "link", "Connect an online service");
      await expect(page).toHaveURL(/#\/settings\/advanced\/online$/);
      await expect(page.getByLabel(/^Address/)).toBeVisible();
      expect(await axe(page), scheme).toEqual([]);
      await context.close();
    }
  });

  test("leaving at the service step goes back to the page", async ({ page }) => {
    await open(page, "/settings/online/setup");
    await press(page, "button", "Continue");
    await press(page, "button", "Not now");
    await expect(page).toHaveURL(/#\/settings\/online$/);
    await expect(page.getByText(/It’s off\./)).toBeVisible();
  });
});
