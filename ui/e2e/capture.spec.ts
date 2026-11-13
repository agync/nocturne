import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const SHOTS = "test-results/screens";
const TAGS = ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"];

const ready = async (page: Page) => {
  await page.goto("/#/quick-capture");
  await page.getByRole("textbox", { name: "Keep something, or ask" }).waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const box = (page: Page) => page.getByRole("textbox", { name: "Keep something, or ask" });

const axe = async (page: Page) =>
  (await new AxeBuilder({ page }).withTags(TAGS).analyze()).violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );

const micArgs = {
  launchOptions: {
    args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  },
  permissions: ["microphone"],
};

test.use(micArgs);

test.describe("the floating window", () => {
  test("opens focused, 560 wide, with both hints and no shell", async ({ page }) => {
    await ready(page);
    await expect(box(page)).toBeFocused();
    const surface = await page.locator("main > div > div").nth(1).boundingBox();
    expect(Math.round(surface?.width ?? 0)).toBe(560);
    await expect(page.getByText("Esc closes")).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Main" })).toHaveCount(0);
    await page.screenshot({ path: `${SHOTS}/capture-empty-light.png` });
  });

  test("a note is kept with Enter, says so, and the window closes by itself", async ({ page }) => {
    await ready(page);
    await box(page).fill("Buy milk");
    await page.screenshot({ path: `${SHOTS}/capture-typed-light.png` });
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept.", { exact: true })).toBeVisible();
    await expect(page.getByText("Nocturne will read it tonight.", { exact: true })).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/capture-kept-light.png` });
    // The browser has no window to close, so the screen leaves for Today.
    await expect(page).toHaveURL(/#\/today$/, { timeout: 4000 });
  });

  test("Undo is there while the pill is shown and holds the window open", async ({ page }) => {
    await ready(page);
    await box(page).fill("Buy milk");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Tab");
    const undo = page.getByRole("button", { name: "Undo" });
    await expect(undo).toBeFocused();
    await page.waitForTimeout(2200);
    await expect(page).toHaveURL(/#\/quick-capture$/);
    await page.keyboard.press("Enter");
    await expect(page.getByText("Taken back. Nothing was kept.")).toBeAttached();
    await expect(box(page)).toBeVisible();
  });

  test("Undo with the pointer, then a second keep, still closes by itself", async ({ page }) => {
    await ready(page);
    await box(page).fill("Buy milk");
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: "Undo" }).click();
    await box(page).fill("Buy eggs");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept.", { exact: true })).toBeVisible();
    await expect(page).toHaveURL(/#\/today$/, { timeout: 4000 });
  });

  test("Ctrl Z takes it back and Escape leaves, straight from the Kept. pill", async ({ page }) => {
    await ready(page);
    await box(page).fill("Buy milk");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept.", { exact: true })).toBeVisible();
    await expect(page.getByRole("main")).toBeFocused();
    await page.keyboard.press("Control+z");
    await expect(box(page)).toBeVisible();
    await box(page).fill("Buy eggs");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/#\/today$/);
  });

  test("Escape leaves the duplicate view", async ({ page }) => {
    await ready(page);
    await box(page).fill("Gran’s new number ends 4471.");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Open it" })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/#\/today$/);
  });

  test("a question goes to the Ask page", async ({ page }) => {
    await ready(page);
    await box(page).fill("When is the dentist?");
    await expect(page.getByRole("button", { name: /^Ask/ })).toHaveCSS(
      "background-color",
      /rgb\(23, 24, 28\)/,
    );
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/ask$/);
  });

  test("Escape closes it", async ({ page }) => {
    await ready(page);
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/#\/today$/);
  });

  test("a duplicate is said kindly", async ({ page }) => {
    await ready(page);
    await box(page).fill("Gran’s new number ends 4471.");
    await page.keyboard.press("Enter");
    await expect(page.getByText(/You already kept this note on/)).toBeVisible();
    await expect(page.getByRole("button", { name: "Open it" })).toBeFocused();
    await page.screenshot({ path: `${SHOTS}/capture-duplicate-light.png` });
  });

  test("pasted web content shows the dashed chip", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await ready(page);
    await page.evaluate(() => {
      const input = document.querySelector("textarea") as HTMLTextAreaElement;
      input.focus();
      const data = new DataTransfer();
      data.setData("text/plain", "Nut-free lunchbox ideas");
      data.setData("text/html", "<p>Nut-free lunchbox ideas</p>");
      input.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
    });
    await box(page).fill("Nut-free lunchbox ideas");
    await expect(page.getByText("From a web page · kept as unconfirmed")).toBeVisible();
    await expect(
      page.getByText("Web pages are never treated as true until you say so."),
    ).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/capture-web-light.png` });
  });
});

test.describe("recording", () => {
  test("hold Space to record, with the level meter and the Ember dot, release to keep", async ({
    page,
  }) => {
    await ready(page);
    await page.keyboard.down(" ");
    await expect(page.getByText("Listening")).toBeVisible();
    await expect(page.getByText("Release Space to keep")).toBeVisible();
    await page.waitForTimeout(1300);
    await expect(page.getByText(/^0:0[1-9]$/)).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/capture-recording-light.png` });
    await page.keyboard.up(" ");
    await expect(page.getByText(/Voice note, 0:0\d\. Reading it now\./)).toBeVisible();
  });

  test("a quick tap of Space does not leave the microphone on", async ({ page }) => {
    await ready(page);
    await page.keyboard.down(" ");
    await page.keyboard.up(" ");
    await page.waitForTimeout(1500);
    await expect(page.getByText("Listening")).toHaveCount(0);
    await expect(box(page)).toBeVisible();
  });

  test("Discard throws the recording away", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Record a voice note" }).click();
    await page.getByRole("button", { name: "Discard" }).click();
    await expect(page.getByText("Listening")).toHaveCount(0);
    await expect(page.getByText(/Voice note/)).toHaveCount(0);
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes in every state in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      locale: "en-GB",
      timezoneId: "Europe/London",
      viewport: { width: 1280, height: 800 },
      permissions: ["microphone"],
    });
    const page = await context.newPage();
    await ready(page);
    expect(await axe(page)).toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/capture-empty-dark.png` });
    await box(page).fill("Buy milk and eggs");
    expect(await axe(page)).toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/capture-typed-dark.png` });
    await page.keyboard.press("Enter");
    await expect(page.getByText("Kept.", { exact: true })).toBeVisible();
    expect(await axe(page)).toEqual([]);
    if (scheme === "dark") await page.screenshot({ path: `${SHOTS}/capture-kept-dark.png` });
    await ready(page);
    await box(page).fill("Gran’s new number ends 4471.");
    await page.keyboard.press("Enter");
    await expect(page.getByText(/You already kept this note/)).toBeVisible();
    expect(await axe(page)).toEqual([]);
    await context.close();
  });
}

test.describe("add a photo or file from the keep bar", () => {
  test("a chosen JPEG is kept and the toast says so", async ({ page }) => {
    await page.goto("/#/today");
    const add = page.getByRole("button", { name: "Add a photo or file" });
    await expect(add).toBeVisible();
    const size = await add.boundingBox();
    expect(Math.round(size?.width ?? 0)).toBe(44);
    expect(Math.round(size?.height ?? 0)).toBe(44);
    const chooser = page.waitForEvent("filechooser");
    await add.click();
    await (await chooser).setFiles({
      name: "lunch.jpg",
      mimeType: "image/jpeg",
      buffer: Buffer.from([0xff, 0xd8, 0xff, 0xd9]),
    });
    await expect(page.getByText(/Kept 1 file\./)).toBeVisible();
    expect(await axe(page)).toEqual([]);
  });
});
