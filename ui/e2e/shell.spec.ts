import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const ready = async (page: Page, hash = "#/today") => {
  await page.goto(`/${hash}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const horizontalOverflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe("wide window", () => {
  test("shows the rail, the bar on top and the kept-today column", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("navigation", { name: "Main" })).toBeVisible();
    await expect(page.getByRole("complementary", { name: "Recently kept" })).toBeVisible();
    const bar = page.getByRole("textbox", { name: "Keep something, or ask" });
    await expect(bar).toBeFocused();
    const box = await bar.boundingBox();
    expect(box?.y ?? 999).toBeLessThan(120);
    expect(await horizontalOverflow(page)).toBeLessThanOrEqual(0);
  });

  test("F6 cycles the bar, the page and the rail", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("textbox", { name: "Keep something, or ask" })).toBeFocused();
    await page.keyboard.press("F6");
    await expect(page.getByRole("main")).toBeFocused();
    await page.keyboard.press("F6");
    await expect(page.getByRole("link", { name: /^Today/ })).toBeFocused();
    await page.keyboard.press("F6");
    await expect(page.getByRole("textbox", { name: "Keep something, or ask" })).toBeFocused();
  });

  test("every control is reachable by keyboard", async ({ page }) => {
    await ready(page, "#/privacy");
    const seen = new Set<string>();
    for (let i = 0; i < 30; i += 1) {
      await page.keyboard.press("Tab");
      const label = await page.evaluate(() => {
        const el = document.activeElement as HTMLElement | null;
        return el
          ? (el.getAttribute("aria-label") ?? el.textContent ?? el.tagName).trim().slice(0, 40)
          : "";
      });
      seen.add(label);
    }
    for (const name of [
      "Keep something, or ask",
      "Hold to talk",
      "What Nocturne remembers",
      "Settings",
    ]) {
      expect(
        [...seen].some((s) => s.includes(name)),
        name,
      ).toBe(true);
    }
  });
});

test.describe("wide window layout", () => {
  test("the status block and the kept-today footer sit at the bottom, as in the design", async ({
    page,
  }) => {
    await ready(page);
    const height = page.viewportSize()?.height ?? 800;
    const status = await page.getByRole("region", { name: "Nocturne status" }).boundingBox();
    const privacy = await page.getByRole("link", { name: "Privacy" }).boundingBox();
    const tonight = await page.getByText(/^Tonight, from about/).boundingBox();
    expect((privacy?.y ?? 0) + (privacy?.height ?? 0)).toBeGreaterThan(height - 40);
    expect((status?.y ?? 0) + (status?.height ?? 0)).toBeGreaterThan(height - 140);
    expect((tonight?.y ?? 0) + (tonight?.height ?? 0)).toBeGreaterThan(height - 40);
  });
});

test.describe("narrow window", () => {
  test.use({ viewport: { width: 400, height: 760 } });

  test("collapses the rail into Menu and docks the bar at the bottom", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("button", { name: "Menu" })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Main" })).toHaveCount(0);
    const bar = await page.getByRole("textbox", { name: "Keep something, or ask" }).boundingBox();
    expect(bar).not.toBeNull();
    expect((bar?.y ?? 0) + (bar?.height ?? 0)).toBeGreaterThan(700);
    expect(await horizontalOverflow(page)).toBeLessThanOrEqual(0);
  });

  test("Menu opens the rail, Escape closes it and focus returns", async ({ page }) => {
    await ready(page);
    const menu = page.getByRole("button", { name: "Menu" });
    await menu.click();
    const dialog = page.getByRole("dialog", { name: "Menu" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("link", { name: "Privacy" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(menu).toBeFocused();
  });
});

test.describe("text at 200%", () => {
  test("the shell reflows without clipping or sideways scrolling", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
    await page.waitForTimeout(150);
    expect(await horizontalOverflow(page)).toBeLessThanOrEqual(0);
    await expect(page.getByRole("button", { name: "Menu" })).toBeVisible();
    const bar = page.getByRole("textbox", { name: "Keep something, or ask" });
    await expect(bar).toBeVisible();
    const box = await bar.boundingBox();
    const viewport = page.viewportSize();
    expect((box?.x ?? 0) + (box?.width ?? 0)).toBeLessThanOrEqual(viewport?.width ?? 0);
  });

  test("controls grow with the text and nothing is clipped", async ({ page }) => {
    await ready(page);
    await page.addStyleTag({ content: "html { font-size: 200% !important; }" });
    await page.waitForTimeout(200);
    const clipped = await page.evaluate(() => {
      const bad: string[] = [];
      for (const el of document.querySelectorAll<HTMLElement>("button, a, textarea, [role=tab]")) {
        const box = el.getBoundingClientRect();
        if (box.width === 0 || box.height === 0) continue;
        if (el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== "auto") {
          bad.push(`${el.tagName} ${el.textContent?.trim().slice(0, 20)}`);
        }
        if (el.tagName === "BUTTON" && box.height < 44 - 1)
          bad.push(`short ${el.textContent?.trim()}`);
      }
      return bad;
    });
    expect(clipped).toEqual([]);
    const bar = await page
      .getByRole("textbox", { name: "Keep something, or ask" })
      .evaluate((el) => {
        const field = el.parentElement as HTMLElement;
        return {
          height: field.getBoundingClientRect().height,
          clipped: field.scrollHeight > field.clientHeight + 1,
        };
      });
    expect(bar.height).toBeGreaterThanOrEqual(44);
    expect(bar.clipped).toBe(false);
  });
});

test.describe("theme", () => {
  test("follows the system and can be forced by data-theme", async ({ browser }) => {
    const dark = await browser.newContext({ colorScheme: "dark" });
    const page = await dark.newPage();
    await ready(page);
    const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(await bg()).toBe("rgb(14, 16, 20)");
    await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
    expect(await bg()).toBe("rgb(238, 235, 228)");
    await dark.close();
  });
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe finds nothing to fix on #/today in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({ colorScheme: scheme });
    const page = await context.newPage();
    await ready(page);
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(
      results.violations.map(
        (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
      ),
    ).toEqual([]);
    await context.close();
  });
}

for (const scheme of ["light", "dark"] as const) {
  test(`axe passes on the narrow menu in ${scheme}`, async ({ browser }) => {
    const context = await browser.newContext({
      colorScheme: scheme,
      viewport: { width: 400, height: 760 },
      locale: "en-GB",
      timezoneId: "Europe/London",
    });
    const page = await context.newPage();
    await ready(page);
    await page.getByRole("button", { name: "Menu" }).click();
    await page.getByRole("dialog", { name: "Menu" }).waitFor();
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(
      results.violations.map(
        (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
      ),
    ).toEqual([]);
    await context.close();
  });
}
