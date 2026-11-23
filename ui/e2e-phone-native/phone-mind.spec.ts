import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "../e2e/axe";

// Choosing a mind on a phone alone, at 412 by 892, light and dark, each screen checked with axe,
// and at 200 per cent text. The service is a stand-in at answers.example. `SHOTS` names a folder
// to keep the screenshots in.

const SHOTS = process.env.SHOTS ?? "test-results/phone-mind-shots";

async function clean(page: Page) {
  const result = await runAxe(page);
  expect(result.violations.map((v) => `${v.id}: ${v.nodes[0]?.target}`)).toEqual([]);
}

async function shot(page: Page, name: string, fullPage = false) {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.screenshot({ path: `${SHOTS}/${name}.png`, fullPage });
}

async function fits(page: Page) {
  const wide = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(wide).toBeLessThanOrEqual(1);
  const clipped = await page.evaluate(() =>
    [...document.querySelectorAll("main button, footer button")]
      .filter((b) => b.scrollHeight > b.clientHeight + 1 || b.scrollWidth > b.clientWidth + 1)
      .map((b) => b.textContent),
  );
  expect(clipped).toEqual([]);
}

async function toDetails(page: Page) {
  await page.getByRole("button", { name: "Choose a mind" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Your service’s details" })).toBeVisible();
}

async function fill(page: Page, key = "abc") {
  await page.getByLabel("API URL").fill("https://answers.example/v1");
  await page.getByLabel("API key").fill(key);
  await page.getByLabel("Model name").fill("reader-small");
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(scheme, () => {
    test.use({ colorScheme: scheme });

    test("Where should Nocturne's mind be?", async ({ page }) => {
      await page.goto("/?m=kept");
      await page.getByRole("button", { name: "Choose a mind" }).click();
      await expect(
        page.getByRole("heading", { name: "Where should Nocturne’s mind be?" }),
      ).toBeVisible();
      await expect(page.getByRole("radio")).toHaveCount(2);
      await clean(page);
      await shot(page, `mind-choose-${scheme}`);
    });

    test("step 1, what goes, and everything sent", async ({ page }) => {
      await page.goto("/?m=kept");
      await page.getByRole("button", { name: "Choose a mind" }).click();
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByText("Nocturne’s mind online · step 1 of 4")).toBeVisible();
      await clean(page);
      await shot(page, `mind-service-${scheme}`);
      await page.getByRole("button", { name: "Everything that is sent" }).click();
      await expect(page.getByText(/Dentist moved to Thursday/)).toBeVisible();
      await clean(page);
      await shot(page, `mind-service-more-${scheme}`, true);
    });

    test("step 2, the check and its states", async ({ page }) => {
      await page.goto("/?m=kept");
      await toDetails(page);
      await clean(page);
      await shot(page, `mind-details-${scheme}`);
      await fill(page, "refused");
      await page.getByRole("button", { name: "Check" }).click();
      await expect(page.getByText("answers.example refused the key.")).toBeVisible();
      await clean(page);
      await shot(page, `mind-details-refused-${scheme}`);
      await page.getByLabel("API key").fill("abc");
      await page.getByRole("button", { name: "Check" }).click();
      await expect(page.getByText("answers.example answered. It works.")).toBeVisible();
      await clean(page);
      await shot(page, `mind-details-works-${scheme}`);
    });

    test("steps 3 and 4, then the reading", async ({ page }) => {
      await page.goto("/?m=kept");
      await toDetails(page);
      await fill(page);
      await page.getByRole("button", { name: "Check" }).click();
      await expect(page.getByText("answers.example answered. It works.")).toBeVisible();
      await page.getByRole("button", { name: "Continue" }).click();
      await page.getByRole("radio", { name: "A number of things read and answered" }).check();
      await expect(page.getByLabel("Things read and answered in a month")).toBeVisible();
      await clean(page);
      await shot(page, `mind-limit-${scheme}`);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(
        page.getByRole("heading", {
          name: "Before you turn this on, here is exactly what happens.",
        }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `mind-consent-${scheme}`);
      await shot(page, `mind-consent-full-${scheme}`, true);
      await page.getByRole("button", { name: "Turn on" }).click();
      await expect(page.getByText("12 of 46 read")).toBeVisible();
      await clean(page);
      await shot(page, `mind-first-page-${scheme}`);
      await page.getByRole("button", { name: "OK" }).click();
      await expect(page.getByText("Written by answers.example")).toBeVisible();
    });

    test("Today with the page the mind wrote", async ({ page }) => {
      await page.goto("/?m=kept&mind=on");
      await expect(page.getByText("Written by answers.example")).toBeVisible();
      await clean(page);
      await shot(page, `mind-today-${scheme}`);
    });

    test("This phone, its mind, and turning off", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&v=thisPhone");
      const group = page.getByRole("region", { name: "Nocturne’s mind online" });
      await expect(group).toContainText("answers.example, online");
      await expect(group).toContainText("3 things read and answered · limit 200");
      await clean(page);
      await shot(page, `mind-this-phone-${scheme}`, true);
      await group.getByRole("button", { name: "Turn off…" }).click();
      await expect(
        page.getByRole("dialog", { name: "Turn off Nocturne’s mind online?" }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `mind-turn-off-${scheme}`);
    });

    test("The key isn't held", async ({ page }) => {
      await page.goto("/?m=kept&mind=needsKey&v=thisPhone");
      await expect(page.getByText("Nocturne can’t read the saved key")).toBeVisible();
      await clean(page);
      await shot(page, `mind-no-key-${scheme}`);
    });

    test("Sent online", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&v=thisPhone");
      await page.getByRole("button", { name: "See what was sent" }).click();
      await expect(page.getByText("Read a thing you kept")).toBeVisible();
      await clean(page);
      await shot(page, `mind-sent-${scheme}`);
    });

    test("Ask with a mind", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&v=ask");
      await page.getByRole("textbox", { name: "Your question" }).fill("What did I mean to ask?");
      await page.getByRole("textbox", { name: "Your question" }).press("Enter");
      await expect(page.getByText("You meant to ask about the swap for the 28th.")).toBeVisible();
      await expect(
        page.getByText("Answered online by your provider (answers.example)"),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Choose a mind" })).toHaveCount(0);
      await expect(page.getByText(/What was sent is listed in/)).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-${scheme}`);
      await page.getByRole("textbox", { name: "Your question" }).fill("no answer for this");
      await page.getByRole("textbox", { name: "Your question" }).press("Enter");
      await expect(
        page.getByText("No answer came this time. Ask again in a moment."),
      ).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-no-answer-${scheme}`);
    });

    test("This phone isn't online", async ({ page }) => {
      const line = "This phone isn’t online. What you keep waits to be read.";
      await page.goto("/?m=kept&mind=on&trouble=offline");
      await expect(page.getByText(line)).toBeVisible();
      await clean(page);
      await shot(page, `mind-today-offline-${scheme}`);
      await page.goto("/?m=kept&mind=on&trouble=offline&v=ask");
      await expect(page.getByText(line)).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-offline-${scheme}`);
    });

    test("The service doesn't answer", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&trouble=unreachable");
      await expect(
        page.getByText(/Written Saturday 21 November\. answers\.example hasn’t answered since /),
      ).toBeVisible();
      await clean(page);
      await shot(page, `mind-today-unreachable-${scheme}`);
      await page.goto("/?m=kept&mind=on&trouble=unreachable&v=ask");
      await expect(page.getByText("Answers are back when answers.example is.")).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-unreachable-${scheme}`);
    });

    test("The key is refused", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&trouble=keyRefused");
      await expect(page.getByText("Your provider refused the key")).toBeVisible();
      await expect(page.getByText(/Until you check it, Ask finds by words/)).toBeVisible();
      await clean(page);
      await shot(page, `mind-today-refused-${scheme}`);
      await page.getByRole("button", { name: "Check the key…" }).click();
      await expect(page.getByRole("heading", { name: "Your service’s details" })).toBeVisible();
    });

    test("No credit, and paused by the limit", async ({ page }) => {
      await page.goto("/?m=kept&mind=on&trouble=noCredit&v=ask");
      await expect(page.getByText("Your provider says there’s no credit left")).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-no-credit-${scheme}`);
      await page.goto("/?m=kept&mind=on&trouble=paused&v=ask");
      await expect(page.getByText("Paused by your limit")).toBeVisible();
      await expect(page.getByText(/Ask finds by words until Tuesday 1 December/)).toBeVisible();
      await clean(page);
      await shot(page, `mind-ask-paused-${scheme}`);
    });
  });
}

test.describe("200 per cent text", () => {
  const big = (page: Page) => page.addStyleTag({ content: "html { font-size: 200% !important; }" });

  test("choose", async ({ page }) => {
    await page.goto("/?m=kept");
    await page.getByRole("button", { name: "Choose a mind" }).click();
    await big(page);
    await expect(
      page.getByRole("heading", { name: "Where should Nocturne’s mind be?" }),
    ).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-choose");
  });

  test("details, limit and consent", async ({ page }) => {
    await page.goto("/?m=kept");
    await toDetails(page);
    await big(page);
    await fill(page);
    await page.getByRole("button", { name: "Check" }).click();
    await expect(page.getByText("answers.example answered. It works.")).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-details");
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("radio", { name: "An amount" }).check();
    await fits(page);
    await shot(page, "text-200-mind-limit");
    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page.getByRole("button", { name: "Turn on" })).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-consent");
  });

  test("this phone and sent online", async ({ page }) => {
    await page.goto("/?m=kept&mind=on&v=thisPhone");
    await big(page);
    await expect(page.getByRole("region", { name: "Nocturne’s mind online" })).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-this-phone");
    await page.getByRole("button", { name: "See what was sent" }).click();
    await expect(page.getByText("Read a thing you kept")).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-sent");
  });

  test("today", async ({ page }) => {
    await page.goto("/?m=kept&mind=on");
    await big(page);
    await expect(page.getByText("Written by answers.example")).toBeVisible();
    await fits(page);
    await shot(page, "text-200-mind-today");
  });
});
