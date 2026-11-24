import { expect, type Page, test } from "@playwright/test";
import { runAxe } from "../e2e/axe";

// The nightly backup on a phone alone, at 412 by 892, light and dark, each screen checked with
// axe, and the key screens at 200 per cent text. The account is scripted in the page; the
// phrase and the recovery key are the scripted ones. `SHOTS` names a folder for the screenshots.

const SHOTS = process.env.SHOTS ?? "test-results/phone-backup-shots";
const PHRASE = "harbour violet pencil maple drift oven";

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

/** Checked, fitted and kept, as one step. */
async function look(page: Page, name: string, fullPage = false) {
  await page.evaluate(() => document.fonts.ready);
  await clean(page);
  await fits(page);
  await shot(page, name, fullPage);
}

async function toStep(page: Page, query: string, step: 1 | 2 | 3) {
  await page.goto(`/?m=kept&v=nightly${query}`);
  await page.getByRole("switch", { name: /Back up every night/ }).click();
  await expect(page.getByText("Backup · step 1 of 3")).toBeVisible();
  await expect(page.getByText(/room for about 412/)).toBeVisible();
  if (step === 1) return;
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByLabel("word 1, harbour")).toBeVisible();
  if (step === 2) return;
  await page.getByRole("button", { name: "Next" }).click();
  await expect(page.getByRole("heading", { name: "Type your phrase" })).toBeVisible();
}

async function toFound(page: Page, query = "") {
  await page.goto(`/?m=first&s=welcome${query}`);
  await page.getByRole("button", { name: "I have a backup" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Bring back your memory" })).toBeVisible();
}

async function bring(page: Page, text = PHRASE) {
  await page.getByLabel(/Your phrase|Recovery key/).fill(text);
  await page.getByRole("button", { name: "Bring it back" }).click();
}

async function toMerge(page: Page, query = "") {
  await page.goto(`/?m=kept&v=thisPhone${query}`);
  await page.getByRole("button", { name: /Restore a backup…/ }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Bring back your memory" })).toBeVisible();
  await bring(page);
  await expect(page.getByRole("button", { name: "Bring them in" })).toBeVisible();
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(scheme, () => {
    test.use({ colorScheme: scheme });

    test("turning on: why, the phrase, typed back, the key, the first backup", async ({ page }) => {
      test.setTimeout(90_000);
      await toStep(page, "", 1);
      await look(page, `backup-why-${scheme}`);
      await page.getByRole("button", { name: "Everything about the backup" }).click();
      await look(page, `backup-why-more-${scheme}`, true);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByLabel("word 1, harbour")).toBeVisible();
      await look(page, `backup-passphrase-${scheme}`);
      await page.getByRole("button", { name: "Use my own phrase instead" }).click();
      await page.getByLabel("Your phrase").fill("sunshine garden");
      await expect(page.getByText(/This phrase is easy to guess/)).toBeVisible();
      await look(page, `backup-own-${scheme}`);
      await page.getByRole("button", { name: "Use Nocturne’s phrase", exact: true }).click();
      await page.getByRole("button", { name: "Next" }).click();
      await page.getByLabel("Your phrase").fill("harbour violet pencil");
      await page.getByRole("button", { name: "Turn on" }).click();
      await expect(page.getByText(/That isn’t the phrase/)).toBeVisible();
      await look(page, `backup-check-${scheme}`);
      await page.getByLabel("Your phrase").fill(PHRASE);
      await page.getByRole("button", { name: "Turn on" }).click();
      await expect(
        page.getByRole("heading", { name: "A recovery key, if you want one" }),
      ).toBeVisible();
      await look(page, `backup-key-${scheme}`);
      await page.getByRole("button", { name: "Not now" }).click();
      await expect(page.getByRole("heading", { name: "Your memory is backed up." })).toBeVisible();
      await look(page, `backup-done-${scheme}`);
    });

    test("turning on: blocked, and the first backup running", async ({ page }) => {
      await toStep(page, "&connect=refused", 1);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByText("Nothing was turned on. Google didn’t allow it.")).toBeVisible();
      await look(page, `backup-blocked-${scheme}`);
      await toStep(page, "&os=ios&connect=turnedOff", 1);
      await expect(page.getByText(/Turn on iCloud for Nocturne/)).toBeVisible();
      await look(page, `backup-blocked-ios-${scheme}`);
      await toStep(page, "&first=slow", 3);
      await page.getByLabel("Your phrase").fill(PHRASE);
      await page.getByRole("button", { name: "Turn on" }).click();
      await page.getByRole("button", { name: "Not now" }).click();
      await expect(page.getByText(/540\sMB of 1.4\sGB/)).toBeVisible();
      await look(page, `backup-first-${scheme}`);
    });

    test("This phone: on, Wi-Fi, stopped, and its sheets", async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto("/?m=kept&v=thisPhone&b=on");
      await expect(page.getByRole("button", { name: /^Backup\s*Last backup/ })).toBeVisible();
      await look(page, `backup-this-phone-row-${scheme}`);
      await page.goto("/?m=kept&v=nightly&b=on");
      await expect(page.getByRole("button", { name: "Back up now" })).toBeVisible();
      await look(page, `backup-on-${scheme}`, true);
      await page.goto("/?m=kept&v=nightly&b=on&os=ios");
      await expect(page.getByRole("button", { name: "Back up now" })).toBeVisible();
      await look(page, `backup-on-ios-${scheme}`, true);
      await page.goto("/?m=kept&v=nightly&b=wifi");
      await expect(page.getByRole("button", { name: "Back up now over mobile data" })).toBeVisible();
      await look(page, `backup-wifi-${scheme}`);
      await page.goto("/?m=kept&v=nightly&b=signedOut");
      await expect(page.getByRole("button", { name: "Choose account" })).toBeVisible();
      await look(page, `backup-stopped-${scheme}`);
      await page.goto("/?m=kept&v=nightly&b=noRoom&os=ios");
      await expect(page.getByRole("button", { name: "Open iCloud settings" })).toBeVisible();
      await look(page, `backup-stopped-ios-${scheme}`);
      await page.goto("/?m=kept&v=nightly&b=another");
      await expect(page.getByText(/Another phone made a newer backup/)).toBeVisible();
      await look(page, `backup-another-${scheme}`);
      await page.goto("/?m=kept&v=nightly&b=on");
      await page.getByRole("button", { name: "Turn off…" }).click();
      await page.getByLabel("Also delete the backup").check();
      await expect(page.getByText(/All seven nights go/)).toBeVisible();
      await look(page, `backup-off-${scheme}`);
      await page.getByRole("button", { name: "Keep it on" }).click();
      await page.getByRole("button", { name: /Make a recovery key…/ }).click();
      await expect(page.getByRole("dialog", { name: "Make a recovery key?" })).toBeVisible();
      await look(page, `backup-makekey-${scheme}`);
      await page.getByRole("button", { name: "Make it" }).click();
      await expect(page.getByRole("heading", { name: "Your new recovery key" })).toBeVisible();
      await look(page, `backup-newkey-${scheme}`);
    });

    test("Today, and Delete everything with a backup", async ({ page }) => {
      await page.goto("/?m=kept&b=noRoom");
      await expect(page.getByText("Your backup has stopped.")).toBeVisible();
      await look(page, `backup-today-${scheme}`);
      await page.goto("/?m=kept&b=on");
      await page.getByRole("button", { name: "This phone" }).click();
      await page.getByRole("button", { name: "Delete everything…" }).click();
      await expect(page.getByText(/Your backup in your Google Drive is deleted too/)).toBeVisible();
      await look(page, `backup-delete-${scheme}`);
    });

    test("restoring onto an empty phone", async ({ page }) => {
      test.setTimeout(90_000);
      await page.goto("/?m=first&s=welcome");
      await page.getByRole("button", { name: "I have a backup" }).click();
      await expect(page.getByRole("heading", { name: "Where is your backup?" })).toBeVisible();
      await look(page, `backup-restore-where-${scheme}`);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByRole("heading", { name: "Bring back your memory" })).toBeVisible();
      await look(page, `backup-restore-found-${scheme}`);
      await page.getByRole("button", { name: "Use the recovery key instead" }).click();
      await bring(page, "K7QM-2XWD-9HRT-P4NC-B8LZ-6VFI");
      await expect(page.getByText(/That recovery key doesn’t open this backup/)).toBeVisible();
      await look(page, `backup-restore-wrong-${scheme}`);
      await page.getByRole("button", { name: "Use the phrase instead" }).click();
      await page.getByLabel("Your phrase").fill(PHRASE);
      await page.getByRole("button", { name: "An earlier night…" }).click();
      await expect(page.getByText("The newest is chosen unless you pick another.")).toBeVisible();
      await look(page, `backup-restore-nights-${scheme}`);
      await page.getByRole("button", { name: "Done" }).click();
      await page.getByRole("button", { name: "Bring it back" }).click();
      await expect(
        page.getByRole("heading", { name: "Your memory is back on this phone." }),
      ).toBeVisible();
      await look(page, `backup-restore-restored-${scheme}`);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(
        page.getByRole("heading", { name: "Make the backups from this phone?" }),
      ).toBeVisible();
      await look(page, `backup-restore-in-use-${scheme}`);
    });

    test("restoring: none, getting, missing", async ({ page }) => {
      await page.goto("/?m=first&s=welcome&found=none");
      await page.getByRole("button", { name: "I have a backup" }).click();
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(
        page.getByRole("heading", { name: "No backup found in this Google account’s Drive." }),
      ).toBeVisible();
      await look(page, `backup-restore-none-${scheme}`);
      await toFound(page, "&restore=slow");
      await bring(page);
      await expect(page.getByText(/650\sMB of 1.4\sGB/)).toBeVisible();
      await look(page, `backup-restore-getting-${scheme}`);
      await toFound(page, "&restore=missing");
      await bring(page);
      await expect(
        page.getByRole("heading", { name: "Part of this backup is missing" }),
      ).toBeVisible();
      await look(page, `backup-restore-missing-${scheme}`);
    });

    test("bringing a backup into a phone with a memory", async ({ page }) => {
      await toMerge(page);
      await look(page, `backup-restore-merge-${scheme}`);
      await page.getByRole("button", { name: "Bring them in" }).click();
      await expect(
        page.getByRole("heading", { name: "Your backup’s things are here." }),
      ).toBeVisible();
      await look(page, `backup-restore-merged-${scheme}`);
      await toMerge(page, "&restore=slow&adding");
      await page.getByRole("button", { name: "Bring them in" }).click();
      await expect(page.getByText("Adding them to the memory on this phone…")).toBeVisible();
      await look(page, `backup-restore-adding-${scheme}`);
    });
  });
}

test.describe("200 per cent text", () => {
  const big = (page: Page) => page.addStyleTag({ content: "html { font-size: 200% !important; }" });

  test("why, the phrase and typing it back", async ({ page }) => {
    await toStep(page, "", 1);
    await big(page);
    await fits(page);
    await shot(page, "text-200-backup-why", true);
    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page.getByLabel("word 1, harbour")).toBeVisible();
    await fits(page);
    await shot(page, "text-200-backup-passphrase", true);
    await page.getByRole("button", { name: "Next" }).click();
    await expect(page.getByRole("heading", { name: "Type your phrase" })).toBeVisible();
    await fits(page);
    await shot(page, "text-200-backup-check", true);
  });

  test("This phone, found and merge", async ({ page }) => {
    await page.goto("/?m=kept&v=nightly&b=on");
    await big(page);
    await expect(page.getByRole("button", { name: "Back up now" })).toBeVisible();
    await fits(page);
    await shot(page, "text-200-backup-on", true);
    await toFound(page);
    await big(page);
    await fits(page);
    await shot(page, "text-200-backup-restore-found", true);
    await toMerge(page);
    await big(page);
    await fits(page);
    await shot(page, "text-200-backup-restore-merge", true);
  });
});
