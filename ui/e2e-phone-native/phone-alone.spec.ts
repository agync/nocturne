import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "../e2e/axe";

// A phone alone at 412 by 892, light and dark, each screen checked with axe, and at 200 per cent
// text. `?m=` picks what the phone holds; `SHOTS` names a folder to keep the screenshots in.

const SHOTS = process.env.SHOTS ?? "test-results/phone-alone-shots";

async function clean(page: Page) {
  const result = await runAxe(page);
  expect(result.violations.map((v) => `${v.id}: ${v.nodes[0]?.target}`)).toEqual([]);
}

async function shot(page: Page, name: string) {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
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

async function toThisPhone(page: Page, m = "fresh") {
  await page.goto(`/?m=${m}`);
  await page.getByRole("button", { name: "This phone" }).click();
  await expect(page.getByRole("region", { name: "Your memory" })).toContainText("412 MB");
}

async function fillBackup(page: Page) {
  await page.getByLabel("Choose a passphrase").fill("copper kettle orbit");
  await page.getByLabel("Type it again").fill("copper kettle orbit");
  await page.getByLabel("I’ve written it down or saved it somewhere safe").check();
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(scheme, () => {
    test.use({ colorScheme: scheme });

    test("Welcome, and the pairing it replaces", async ({ page }) => {
      await page.goto("/?m=first&s=welcome");
      await expect(
        page.getByRole("heading", { name: "Nocturne remembers your day" }),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Start on this phone" })).toBeVisible();
      await expect(page.getByRole("button", { name: "I have a backup" })).toBeVisible();
      await clean(page);
      await shot(page, `after-welcome-${scheme}`);
      await page.getByRole("button", { name: "Connect to my computer" }).click();
      await expect(
        page.getByRole("heading", { name: "Nocturne lives on your computer" }),
      ).toBeVisible();
      await shot(page, `before-welcome-${scheme}`);
    });

    test("Your memory lives on this phone, then the offer", async ({ page }) => {
      await page.goto("/?m=first&s=welcome");
      await page.getByRole("button", { name: "Start on this phone" }).click();
      await expect(
        page.getByRole("heading", { name: "Your memory lives on this phone" }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-start-${scheme}`);
      await page.getByRole("button", { name: "Start" }).click();
      await expect(
        page.getByText(/Nocturne can show what’s in this phone’s calendar/),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-offer-${scheme}`);
    });

    test("Today on the first day", async ({ page }) => {
      await page.goto("/?m=fresh");
      await expect(page.getByText("Your memory is on this phone")).toBeVisible();
      await expect(
        page.getByRole("region", { name: "Your page needs a mind to write it." }),
      ).toBeVisible();
      await expect(page.getByText("Nothing in your calendar today.")).toBeVisible();
      await expect(page.getByText(/Nothing kept yet today/)).toBeVisible();
      await clean(page);
      await shot(page, `after-today-${scheme}`);
    });

    test("Today after 30 days with no backup", async ({ page }) => {
      await page.goto("/?m=reminder");
      await expect(
        page.getByText("No backup yet. If this phone is lost, what you kept goes with it."),
      ).toBeVisible();
      await expect(page.getByRole("heading", { name: "Kept today · 2" })).toBeVisible();
      await expect(page.getByText("waiting to be read")).toBeVisible();
      await clean(page);
      await shot(page, `after-reminder-${scheme}`);
    });

    test("the journal with no mind", async ({ page }) => {
      await page.goto("/?m=kept&v=journal");
      await expect(page.getByText("Reflections need a mind.")).toBeVisible();
      await expect(page.getByText("waiting to be read")).toBeVisible();
      await clean(page);
      await shot(page, `after-journal-${scheme}`);
    });

    test("Ask, nothing kept yet, then found by words", async ({ page }) => {
      await page.goto("/?m=fresh&v=ask");
      await expect(
        page.getByText("Nothing kept yet. What you keep can be found here by its words."),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-ask-${scheme}`);
      await page.goto("/?m=kept&v=ask");
      await page.getByRole("textbox", { name: "Find something you kept" }).fill("swap");
      await expect(page.getByText("Found by their words. Answers need a mind.")).toBeVisible();
      await expect(page.locator("mark").first()).toHaveText("swap");
      await clean(page);
      await shot(page, `after-found-${scheme}`);
    });

    test("Give Nocturne a mind", async ({ page }) => {
      await page.goto("/?m=fresh");
      await page.getByRole("button", { name: "Choose a mind" }).click();
      await expect(page.getByRole("heading", { name: "Give Nocturne a mind" })).toBeVisible();
      await clean(page);
      await shot(page, `after-mind-${scheme}`);
    });

    test("This phone", async ({ page }) => {
      await toThisPhone(page);
      await expect(
        page.getByRole("button", { name: /Save a backup….*No backup yet/ }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-this-phone-${scheme}`);
      await page.goto("/?v=thisPhone");
      await expect(page.getByRole("region", { name: "Connected to" })).toBeVisible();
      await shot(page, `before-this-phone-${scheme}`);
    });

    test("Save a backup, then saved", async ({ page }) => {
      await toThisPhone(page, "kept");
      await page.getByRole("button", { name: /Save a backup…/ }).click();
      await fillBackup(page);
      await expect(page.getByRole("button", { name: "Save the backup…" })).not.toHaveAttribute(
        "aria-disabled",
        "true",
      );
      await clean(page);
      await shot(page, `after-backup-${scheme}`);
      await page.getByRole("button", { name: "Save the backup…" }).click();
      await expect(page.getByRole("heading", { name: "Your backup is saved." })).toBeVisible();
      await clean(page);
      await shot(page, `after-saved-${scheme}`);
    });

    test("Delete everything on this phone", async ({ page }) => {
      await toThisPhone(page, "kept");
      await page.getByRole("button", { name: "Delete everything…" }).click();
      await page.getByLabel("Type “delete everything” to go ahead").fill("delete everything");
      await clean(page);
      await shot(page, `after-delete-${scheme}`);
    });

    test("an item's page with its private switch", async ({ page }) => {
      await page.goto("/?m=kept");
      await page.getByRole("button", { name: /Ask about the swap/ }).click();
      await expect(page.getByRole("switch", { name: /Private/ })).toBeVisible();
      await clean(page);
      await shot(page, `after-item-${scheme}`);
    });

    test("keeping, kept, and no room", async ({ page }) => {
      await page.goto("/?m=fresh");
      await page.getByRole("button", { name: "Keep" }).click();
      await page.getByLabel(/Type something to keep/).fill("Bins go out on Tuesday");
      await page.getByRole("button", { name: "Keep", exact: true }).click();
      await expect(
        page.getByText("It’s on today’s journal, and Ask can find it by its words."),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-kept-${scheme}`);
      await page.goto("/?m=fresh&v=keep");
      await page.getByLabel(/Type something to keep/).fill("no room left");
      await page.getByRole("button", { name: "Keep", exact: true }).click();
      await expect(
        page.getByText("There wasn’t room to keep this. Free some room and try again."),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-keep-full-${scheme}`);
    });

    test("the line before the microphone prompt", async ({ page }) => {
      await page.goto("/?m=fresh&v=keep");
      await page.getByRole("button", { name: /Record/ }).click();
      await expect(page.getByText(/^Next, Android asks/)).toBeVisible();
      await clean(page);
      await shot(page, `after-voice-ask-${scheme}`);
    });

    test("Replace what's on this phone?", async ({ page }) => {
      await page.goto("/?m=first&s=welcome");
      await page.getByRole("button", { name: "I have a backup" }).click();
      await page.getByRole("button", { name: "Choose the file" }).click();
      await page.getByLabel("Passphrase for this backup").fill("already here");
      await page.getByRole("button", { name: "Bring it back" }).click();
      await expect(
        page.getByRole("heading", { name: "Replace what’s on this phone?" }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-replace-${scheme}`);
    });

    test("the memory can't be opened, then bringing back a backup", async ({ page }) => {
      await page.goto("/?m=damaged");
      await expect(
        page.getByRole("heading", { name: "Nocturne couldn’t open your memory on this phone." }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-damaged-${scheme}`);
      await page.getByRole("button", { name: "I have a backup" }).click();
      await page.getByRole("button", { name: "Choose the file" }).click();
      await page.getByLabel("Passphrase for this backup").fill("correct horse battery");
      await clean(page);
      await shot(page, `after-restore-${scheme}`);
      await page.getByRole("button", { name: "Bring it back" }).click();
      await expect(
        page.getByRole("heading", { name: "Your memory is back on this phone." }),
      ).toBeVisible();
      await clean(page);
      await shot(page, `after-restored-${scheme}`);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByRole("heading", { name: "Kept today · 2" })).toBeVisible();
    });
  });
}

test.describe("200 per cent text", () => {
  for (const [name, url] of [
    ["welcome", "/?m=first&s=welcome"],
    ["today", "/?m=reminder"],
    ["ask", "/?m=fresh&v=ask"],
    ["this-phone", "/?m=fresh&v=thisPhone"],
    ["damaged", "/?m=damaged"],
  ] as const) {
    test(name, async ({ page }) => {
      await page.goto(url);
      await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
      await expect(page.locator("main")).toBeVisible();
      await fits(page);
      await shot(page, `text-200-${name}`);
    });
  }
});
