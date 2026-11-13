import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const bar = (page: Page) => page.getByRole("textbox", { name: "Keep something, or ask" });

const ready = async (page: Page, hash = "#/ask") => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

/** Asks from the keep/ask bar, as a person does, and waits for the answer to settle. */
const ask = async (page: Page, question: string) => {
  await bar(page).fill(question);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(`“${question}”`);
};

const settled = (page: Page) => expect(page.locator("article[aria-busy='false']")).toBeVisible();

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("the Ask page", () => {
  test("starts with examples, not a blank page", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      "Ask about anything you’ve kept.",
    );
    await expect(page.getByRole("button", { name: "What is Gran’s new number?" })).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/ask-empty-light.png` });
  });

  test("a question from the bar becomes the headline, the answer arrives with its receipt", async ({
    page,
  }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await expect(page).toHaveURL(/#\/ask$/);
    await settled(page);
    await expect(page.locator("article p[class*=answer]")).toHaveText(
      "Gran’s new number ends 4471.",
    );
    await expect(page.getByRole("button", { name: /^Source: note/ })).toBeVisible();
    await expect(
      page.getByText("Answered from 1 thing you kept · nothing sent to the internet"),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Read aloud" })).toHaveCount(0);
    await page.screenshot({ path: `${SHOTS}/ask-answer-light.png` });
  });

  test("the answer is announced once, on completion, in a polite status", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    const announcer = page.locator("[role='status'][aria-atomic='true']").filter({
      hasText: "Answer ready.",
    });
    await expect(announcer).toHaveText("Answer ready. Gran’s new number ends 4471.");
    await expect(announcer).toHaveAttribute("aria-live", "polite");
    // The sentence itself sits in no live region.
    expect(
      await page.evaluate(() =>
        document.querySelector("article p[class*=answer]")?.closest("[aria-live]"),
      ),
    ).toBeNull();
  });

  test("a second question stacks the first under Earlier", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    await ask(page, "What did we decide about half term?");
    await settled(page);
    const earlier = page.getByRole("region", { name: "Earlier" });
    await expect(earlier.getByText("“What is Gran’s new number?”")).toBeVisible();
    await expect(earlier.getByText("Gran’s new number ends 4471.")).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/ask-earlier-light.png` });
  });

  test("the follow-up field asks without leaving the page", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    await page.getByRole("textbox", { name: "Ask a follow-up" }).fill("half term");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("“half term”");
  });

  test("not sure shows the near matches and links to a wider search", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "When is my appointment?");
    await expect(
      page.locator("article").getByText(/I’m not sure\. Nothing says that outright/),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: "Search everything you’ve kept" })).toHaveAttribute(
      "href",
      "#/memories?q=When+is+my+appointment%3F",
    );
    await page.screenshot({ path: `${SHOTS}/ask-not-sure-light.png` });
  });

  test("nothing found offers a photo and the let-go search", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "Where is my passport?");
    await expect(
      page.locator("article").getByText("Nothing you’ve kept answers that."),
    ).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/ask-nothing-light.png` });
    await page.getByRole("button", { name: "Search let go" }).click();
    await expect(page).toHaveURL(/#\/fading\?show=letGo$/);
  });

  test("That’s wrong opens an inline correction and keyboard returns focus", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    const wrong = page.getByRole("button", { name: "That’s wrong" });
    await wrong.focus();
    await page.keyboard.press("Enter");
    const field = page.getByRole("textbox", { name: "What is right? Write it as one sentence." });
    await expect(field).toBeFocused();
    await page.screenshot({ path: `${SHOTS}/ask-correction-light.png` });
    await page.keyboard.press("Escape");
    await expect(wrong).toBeFocused();
    await wrong.press("Enter");
    await field.fill("Gran’s new number ends 5582.");
    await page.getByRole("button", { name: "Save the correction" }).click();
    await expect(page.getByText("Corrected. The old answer is kept as history.")).toBeVisible();
    await expect(page.getByText("Corrected. The old answer is kept as history.")).toBeFocused();
  });

  test("Keep forever says so and Forget this opens the dialog and returns focus", async ({
    page,
  }) => {
    await ready(page, "#/today");
    await ask(page, "What did we decide about half term?");
    await settled(page);
    await page.getByRole("button", { name: "Keep forever" }).click();
    await expect(page.getByText("Kept forever. It will not fade.")).toBeVisible();
    const forget = page.getByRole("button", { name: "Forget this…" });
    await forget.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(forget).toBeFocused();
  });

  test("opening a receipt and coming back finds the answer where it was", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    await ask(page, "What did we decide about half term?");
    await settled(page);
    await page
      .locator("article")
      .getByRole("button", { name: /^Source:/ })
      .click();
    await expect(page).toHaveURL(/#\/items\//);
    await page.goBack();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      "“What did we decide about half term?”",
    );
    await expect(page.locator("article p[class*=answer]")).toContainText("Lake District");
    await expect(
      page.getByRole("region", { name: "Earlier" }).getByText("Gran’s new number ends 4471."),
    ).toBeVisible();
  });

  test("an example moves focus to the question", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "What is Gran’s new number?" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toBeFocused();
  });

  test("a receipt opens its original", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    await page.getByRole("button", { name: /^Source: note/ }).click();
    await expect(page).toHaveURL(/#\/items\/item-gran$/);
  });
});

test.describe("a narrow window", () => {
  test.use({ viewport: { width: 400, height: 760 } });

  test("reflows: no sideways scroll, the shell bar is the only input, receipts stay in view", async ({
    page,
  }) => {
    await ready(page, "#/today");
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await expect(page.getByRole("textbox", { name: "Ask a follow-up" })).toBeHidden();
    await expect(bar(page)).toBeVisible();
    await expect(page.getByRole("button", { name: /^Source:/ })).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/ask-answer-narrow.png` });
  });

  test("not sure and nothing found fit too", async ({ page }) => {
    await ready(page, "#/today");
    await ask(page, "When is my appointment?");
    await expect(page.locator("article").getByText(/I’m not sure/)).toBeVisible();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await page.screenshot({ path: `${SHOTS}/ask-not-sure-narrow.png` });
    await ask(page, "Where is my passport?");
    await expect(
      page.locator("article").getByText("Nothing you’ve kept answers that."),
    ).toBeVisible();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
  });
});

test("answer survives 200% text without clipping", async ({ page }) => {
  await ready(page, "#/today");
  await ask(page, "What is Gran’s new number?");
  await settled(page);
  await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
  await page.waitForTimeout(200);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.screenshot({ path: `${SHOTS}/ask-answer-text-200.png` });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on every state in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await ready(page);
    expect(await axe(page), "empty").toEqual([]);
    await ask(page, "What is Gran’s new number?");
    await settled(page);
    expect(await axe(page), "answer").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/ask-answer-dark.png` });
    await page.getByRole("button", { name: "That’s wrong" }).click();
    expect(await axe(page), "correction").toEqual([]);
    await page.keyboard.press("Escape");
    await ask(page, "When is my appointment?");
    await expect(page.locator("article").getByText(/I’m not sure/)).toBeVisible();
    expect(await axe(page), "not sure").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/ask-not-sure-dark.png` });
    await ask(page, "Where is my passport?");
    await expect(
      page.locator("article").getByText("Nothing you’ve kept answers that."),
    ).toBeVisible();
    expect(await axe(page), "nothing").toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/ask-nothing-dark.png` });
    await context.close();
  });
}
