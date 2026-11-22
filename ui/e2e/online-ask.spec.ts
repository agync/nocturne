import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "./axe";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];
const QUESTION = "What is Gran’s new number?";

const bar = (page: Page) => page.getByRole("textbox", { name: "Keep something, or ask" });
const where = (page: Page) => page.getByRole("radiogroup", { name: "Answer this" });

const ready = async (page: Page, hash: string, scenario: string | null = null) => {
  await page.goto(`/${scenario ? `?mockOnline=${scenario}` : ""}${hash}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const ask = async (page: Page, question: string) => {
  await bar(page).fill(question);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(`“${question}”`);
  await expect(page.locator("article[aria-busy='false']")).toBeVisible();
};

const axe = async (page: Page) =>
  (await runAxe(page, (b) => b.withTags(TAGS))).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const goto = (page: Page, hash: string) =>
  page.evaluate((h) => {
    window.location.hash = h;
  }, hash);

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("with Online answers off", () => {
  test("the bar and the answer line are as they were", async ({ page }) => {
    await ready(page, "#/today");
    await bar(page).fill(QUESTION);
    await expect(where(page)).toHaveCount(0);
    await page.keyboard.press("Enter");
    await expect(page.locator("article[aria-busy='false']")).toBeVisible();
    await expect(page.getByText("Answered from 1 thing you kept")).toBeVisible();
    await expect(page.getByText(/Answered online|Answered on this computer/)).toHaveCount(0);
    await expect(where(page)).toHaveCount(0);
    expect(await axe(page)).toEqual([]);
  });

  test("Privacy says nothing about online answers, and Advanced shows no online block", async ({
    page,
  }) => {
    await ready(page, "#/privacy");
    await expect(page.getByText("Sent for online answers")).toHaveCount(0);
    expect(await axe(page)).toEqual([]);
    await ready(page, "#/settings/advanced");
    await expect(page.getByRole("heading", { name: "Online answers" })).toHaveCount(0);
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`what was sent is accessible in ${scheme}`, async ({ browser }) => {
      const context = await browser.newContext({
        colorScheme: scheme,
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1280, height: 800 },
      });
      const page = await context.newPage();
      await ready(page, "#/settings/online/sent");
      await expect(page.getByRole("heading", { level: 1, name: "Sent online" })).toBeVisible();
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/sent-${scheme}.png` });
      await context.close();
    });
  }
});

test.describe("with Online answers on", () => {
  test("Answer this is reached before sending, and the choice is for one question", async ({
    page,
  }) => {
    await ready(page, "#/today", "on");
    await bar(page).fill(QUESTION);
    const online = page.getByRole("radio", { name: "Online · api.example.com" });
    await expect(online).toBeChecked();
    await bar(page).focus();
    await page.keyboard.press("Shift+Tab");
    await expect(online).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("radio", { name: "This computer" })).toBeChecked();
    await page.getByRole("button", { name: /^Ask/ }).click();
    await expect(page.locator("article[aria-busy='false']")).toBeVisible();
    await expect(
      page.getByText(
        "Answered on this computer from 1 thing you kept · as you chose for this question",
      ),
    ).toBeVisible();
    await bar(page).fill("When is the dentist?");
    await expect(
      page.getByRole("search").getByRole("radio", { name: "Online · api.example.com" }),
    ).toBeChecked();
  });

  for (const scheme of ["light", "dark"] as const) {
    test(`an online answer is marked, and every screen is accessible in ${scheme}`, async ({
      browser,
    }) => {
      const context = await browser.newContext({
        colorScheme: scheme,
        locale: "en-GB",
        timezoneId: "Europe/London",
        viewport: { width: 1280, height: 800 },
      });
      const page = await context.newPage();
      await ready(page, "#/today", "on");
      await ask(page, QUESTION);
      const mark = page.getByText(
        "Answered online by your provider (api.example.com) from 1 thing you kept",
      );
      await expect(mark).toBeVisible();
      const announcer = page.locator("[role='status'][aria-atomic='true']").filter({
        hasText: "Answer ready.",
      });
      await expect(announcer).toContainText("Answered online by your provider, api.example.com.");
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/ask-online-${scheme}.png` });

      await page.getByRole("link", { name: "See what was sent" }).click();
      await expect(page.getByRole("heading", { level: 1, name: "Sent online" })).toBeVisible();
      await expect(page.getByRole("region", { name: "Entry" })).toContainText(QUESTION);
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/sent-entry-${scheme}.png` });

      await goto(page, "#/privacy");
      await page.getByRole("region", { name: "Online answers" }).waitFor();
      expect(await axe(page)).toEqual([]);
      await page.screenshot({ path: `${SHOTS}/privacy-online-${scheme}.png` });

      await goto(page, "#/settings/advanced");
      await page.getByRole("region", { name: "Online answers" }).waitFor();
      expect(await axe(page)).toEqual([]);
      await context.close();
    });
  }

  test.describe("narrow", () => {
    test.use({ viewport: { width: 400, height: 760 } });
    test("an online answer fits 400 wide", async ({ page }) => {
      await ready(page, "#/today", "on");
      await ask(page, QUESTION);
      await expect(page.getByText(/^Answered online by your provider/)).toBeVisible();
      expect(await overflow(page)).toBeLessThanOrEqual(0);
    });
  });
});

test.describe("the notice for someone who agreed before", () => {
  test("holds the question, is axe clean in light and dark, and Continue sends it", async ({
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
      await ready(page, "#/today", "changed");
      await bar(page).fill(QUESTION);
      await page.keyboard.press("Enter");
      await expect(
        page.getByRole("heading", { level: 1, name: "Online answers has changed" }),
      ).toBeFocused();
      await expect(page.getByText("Your question waits until you continue.")).toBeVisible();
      expect(await axe(page), scheme).toEqual([]);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      await page.screenshot({ path: `${SHOTS}/online-changed-${scheme}.png`, fullPage: true });
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByRole("heading", { level: 1 })).toHaveText(`“${QUESTION}”`);
      await expect(
        page.getByText("Answered online by your provider (api.example.com) from 1 thing you kept"),
      ).toBeVisible();
      await context.close();
    }
  });
});
