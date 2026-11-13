import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
/** The desktop device profile reports Windows, where Most private is not offered yet; these run as Linux. */
const LINUX =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

test.use({ userAgent: LINUX });

/** The mock computer starts in the named state, and its scripted steps take `step` milliseconds. */
const open = async (page: Page, path: string, scenario: string, extra = "", step = 250) => {
  await page.goto(`/?mockAway=${scenario}&mockStep=${step}${extra}#${path}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

/** Presses a control the way the keyboard does: focus it, then Enter. */
const press = async (page: Page, role: "button" | "link" | "radio", name: string | RegExp) => {
  const target = page.getByRole(role, { name, exact: typeof name === "string" });
  await target.focus();
  await expect(target).toBeFocused();
  await page.keyboard.press(role === "radio" ? "Space" : "Enter");
};

test.describe("S1, the page, in each state", () => {
  const states: Array<[string, string, string | RegExp]> = [
    ["off", "off", /It’s off\. Nothing is downloaded/],
    ["on", "on", "Simple"],
    ["in-use", "inUse", "Using Nocturne now"],
    ["relink", "relink", /river · copper · meadow · thistle/],
    ["problem", "problem", /Cloudflare didn’t answer/],
  ];
  for (const [name, scenario, text] of states) {
    test(`${name}: axe clean in light and dark, no sideways scroll`, async ({ browser }) => {
      for (const scheme of ["light", "dark"] as const) {
        const context = await browser.newContext({
          colorScheme: scheme,
          userAgent: LINUX,
          locale: "en-GB",
          timezoneId: "Europe/London",
          viewport: { width: 1280, height: 800 },
        });
        const page = await context.newPage();
        await open(page, "/settings/away", scenario);
        await expect(page.getByText(text).first()).toBeVisible();
        expect(await axe(page), `${name} ${scheme}`).toEqual([]);
        expect(await overflow(page)).toBeLessThanOrEqual(0);
        await page.screenshot({ path: `${SHOTS}/away-${name}-${scheme}.png` });
        await context.close();
      }
    });
  }

  test("narrow: nothing scrolls sideways at 400 wide", async ({ browser }) => {
    const context = await browser.newContext({
      userAgent: LINUX,
      viewport: { width: 400, height: 760 },
    });
    const page = await context.newPage();
    await open(page, "/settings/away", "on");
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await context.close();
  });

  test("advanced details open and close from the keyboard, and stay axe clean", async ({
    page,
  }) => {
    await open(page, "/settings/away", "on");
    await press(page, "button", "Show advanced details");
    await expect(page.getByRole("region", { name: "Advanced details" })).toContainText(
      "Cloudflare quick tunnel",
    );
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Hide advanced details");
    await expect(page.getByRole("region", { name: "Advanced details" })).toHaveCount(0);
  });

  test("the switch, the select and the confirm all work from the keyboard", async ({ page }) => {
    await open(page, "/settings/away", "on");
    const select = page.getByRole("combobox", { name: "Turn off by itself" });
    await select.focus();
    await select.selectOption({ label: "After a day" });
    await expect(page.getByText("Saved.")).toBeVisible();
    await press(page, "button", "Change…");
    const dialog = page.getByRole("dialog", { name: "Switch to Most private?" });
    await expect(dialog).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Change…" })).toBeFocused();
  });
});

test.describe("turning it on, by keyboard", () => {
  test("Simple: choose, download, start, add a phone by scanning, allow it", async ({ page }) => {
    await open(page, "/settings/away", "off");
    await press(page, "button", "Set up…");
    await expect(page.getByRole("radio", { name: /Simple/ })).toBeChecked();
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Continue");
    await expect(page.getByRole("heading", { name: /Download it now\?/ })).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Download");
    await expect(page.getByRole("progressbar")).toBeVisible();
    expect(await axe(page)).toEqual([]);

    await expect(page.getByRole("heading", { name: "Add a phone", level: 1 })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole("img", { name: /Code to scan/ })).toBeVisible();
    await expect(page.getByRole("timer")).toContainText("left");
    expect(await axe(page)).toEqual([]);

    const dialog = page.getByRole("alertdialog", { name: "A phone wants to connect" });
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog).toBeFocused();
    await expect(dialog.getByRole("listitem")).toHaveText(["harbour", "maple", "lantern"]);
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Allow");
    await expect(page.getByText("Pixel is added.")).toBeVisible();
    await press(page, "button", "Done");
    await expect(page.getByRole("switch", { name: "Away from home is on" })).toBeChecked();
    await expect(page.getByText("Phones · 1")).toBeVisible();
    expect(await axe(page)).toEqual([]);
  });

  test("a phone that types the code is checked with the same three words", async ({ page }) => {
    await open(page, "/settings/away/add", "onAlone", "&mockPhone=typed");
    await expect(
      page.getByRole("list", { name: "Code to type, in four groups of six" }).getByRole("listitem"),
    ).toHaveCount(4);
    const dialog = page.getByRole("alertdialog", { name: "A phone wants to connect" });
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await press(page, "button", "Don’t allow");
    await expect(page.getByText("That phone wasn’t added.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Make a new code" })).toBeVisible();
    expect(await axe(page)).toEqual([]);
  });

  test("Most private: sign in and secure links wait, then the phone app step", async ({ page }) => {
    await open(page, "/settings/away/setup", "signIn", "", 1500);
    await expect(
      page.getByRole("heading", { name: /Sign in to Tailscale in your browser/ }),
    ).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await expect(
      page.getByRole("heading", { name: "Allow secure links in Tailscale." }),
    ).toBeVisible({ timeout: 10_000 });
    expect(await axe(page)).toEqual([]);
    await expect(page.getByRole("heading", { name: "Get Tailscale on your phone." })).toBeVisible({
      timeout: 10_000,
    });
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Done");
    await expect(page.getByRole("heading", { name: "Add a phone", level: 1 })).toBeVisible();
  });

  test("downloading: pause, resume and cancel work, and the page is axe clean", async ({
    page,
  }) => {
    await open(page, "/settings/away/setup", "downloading", "", 60000);
    await press(page, "button", "Pause");
    await expect(page.getByRole("button", { name: "Resume" })).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await press(page, "button", "Cancel");
    await expect(page.getByRole("heading", { name: "Away from home", level: 1 })).toBeVisible();
    await expect(page.getByText("Off", { exact: true }).first()).toBeVisible();
  });
});

test.describe("phones and activity", () => {
  for (const scheme of ["light", "dark"] as const) {
    test(`phones and activity are axe clean in ${scheme}`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: scheme,
        userAgent: LINUX,
        locale: "en-GB",
        timezoneId: "Europe/London",
      });
      const page = await context.newPage();
      await open(page, "/settings/away/phones", "inUse");
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/away-phones-${scheme}.png` });
      await press(page, "button", "Remove Work phone");
      await expect(page.getByRole("dialog", { name: "Remove Work phone?" })).toBeVisible();
      expect(await axe(page)).toEqual([]);
      await page.keyboard.press("Escape");
      await open(page, "/settings/away/activity", "on");
      await expect(page.getByText(/Someone tried 5 wrong pairing codes/)).toBeVisible();
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/away-activity-${scheme}.png` });
      await context.close();
    });
  }

  test("rename a phone from the keyboard and see it in the list", async ({ page }) => {
    await open(page, "/settings/away/phones", "on");
    await press(page, "button", "Rename Work phone");
    const field = page.getByRole("textbox", { name: "New name for Work phone" });
    await expect(field).toBeFocused();
    await field.fill("Spare");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Rename Spare" })).toBeFocused();
  });
});

test.describe("privacy and the rail", () => {
  for (const scheme of ["light", "dark"] as const) {
    test(`privacy with Away from home on is axe clean in ${scheme}`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: scheme,
        userAgent: LINUX,
        locale: "en-GB",
        timezoneId: "Europe/London",
      });
      const page = await context.newPage();
      await open(page, "/privacy", "on");
      await expect(page.getByRole("region", { name: "Away from home" })).toContainText(
        "through Cloudflare",
      );
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/away-privacy-${scheme}.png` });
      await context.close();
    });
  }

  test("the rail says it is on, then follows the phone using it", async ({ page }) => {
    await open(page, "/today", "on");
    const rail = page.getByRole("navigation", { name: "Main" });
    await expect(rail.getByRole("link", { name: "Away from home is on" })).toBeVisible();
    await expect(rail.getByText("Sent to your phones")).toBeVisible();
    await press(page, "link", "Away from home is on");
    await expect(page.getByRole("heading", { name: "Away from home", level: 1 })).toBeVisible();
  });

  test("the back link and the Simple label carry no Trial badge", async ({ page }) => {
    await open(page, "/settings/away", "on");
    await expect(page.getByRole("link", { name: "Settings" }).first()).toBeVisible();
    await expect(page.getByText("Trial", { exact: true })).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Turn off and remove all phones…" }),
    ).toBeVisible();
  });

  test("the Settings row opens Away from home", async ({ page }) => {
    await open(page, "/settings", "on");
    await press(page, "link", /Open Away from home, On · Simple · 2 phones/);
    await expect(page.getByRole("switch", { name: "Away from home is on" })).toBeVisible();
  });
});

test.describe("on Windows", () => {
  test.use({
    userAgent:
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36",
  });

  test("only Simple is offered until Most private is proven there", async ({ page }) => {
    await open(page, "/settings/away/setup", "off");
    await expect(page.getByRole("radio", { name: /Simple/ })).toBeChecked();
    await expect(page.getByRole("radio", { name: /Most private/ })).toHaveCount(0);
    expect(await axe(page)).toEqual([]);
  });
});
