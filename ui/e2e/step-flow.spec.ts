import { expect, type Page, test } from "@playwright/test";

const SIZES = [
  { width: 1280, height: 800 },
  { width: 900, height: 600 },
];

const ONLINE = "/?mockOnline=off&mockCheckMs=150#/settings/online/setup";
const CONNECT = "/?mockOnline=off&mockCheckMs=150#/settings/advanced/online?from=online";
const AWAY = "/?mockAway=off&mockStep=250#/settings/away/setup";

const open = async (page: Page, url: string) => {
  await page.goto(url);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.evaluate(() => document.fonts.ready);
};

const bar = (page: Page) => page.locator("[data-action-bar]");

/** The element that scrolls under the bar: its nearest ancestor that scrolls, or the page. */
const scroller = (page: Page) =>
  bar(page).evaluateHandle((el) => {
    for (let node = el.parentElement; node; node = node.parentElement) {
      const overflow = getComputedStyle(node).overflowY;
      if (overflow === "auto" || overflow === "scroll") return node;
    }
    return document.scrollingElement ?? document.documentElement;
  });

/** The button is whole on screen and the bar sits on the bottom edge of the window. */
async function expectBarInView(page: Page, name: string) {
  const size = page.viewportSize();
  const box = await page.getByRole("button", { name, exact: true }).boundingBox();
  const frame = await bar(page).boundingBox();
  expect(size && box && frame).toBeTruthy();
  if (!size || !box || !frame) return;
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(size.height);
  expect(frame.y + frame.height).toBeGreaterThanOrEqual(size.height - 1);
  expect(frame.y + frame.height).toBeLessThanOrEqual(size.height + 1);
}

for (const size of SIZES) {
  test.describe(`the bar at ${size.width} by ${size.height}`, () => {
    test.use({ viewport: size });

    test("Online answers step 1 keeps Continue and its line in view", async ({ page }) => {
      await open(page, ONLINE);
      await expect(
        page.getByRole("heading", { level: 1, name: /Have your answers written/ }),
      ).toBeVisible();
      await expectBarInView(page, "Continue");
      await expect(bar(page).getByText("Nothing is turned on until the last step.")).toBeVisible();
      await expect(page.getByRole("button", { name: "Not now" })).toBeVisible();
    });

    test("Away from home step 1 keeps Continue and its line in view", async ({ page }) => {
      await open(page, AWAY);
      await expect(
        page.getByRole("heading", { level: 1, name: /Use Nocturne from your phone/ }),
      ).toBeVisible();
      await expectBarInView(page, "Continue");
      await expect(
        bar(page).getByText("Nothing is downloaded or turned on until the next step."),
      ).toBeVisible();
    });

    test("the bar stays put while the words scroll under it, and fades only then", async ({
      page,
    }) => {
      await open(page, ONLINE);
      await page
        .getByRole("button", { name: "Everything that is sent, and what stays here" })
        .click();
      await expectBarInView(page, "Continue");
      await expect(bar(page)).toHaveAttribute("data-more", "true");
      await (await scroller(page)).evaluate((el) => {
        el.scrollTop = el.scrollHeight;
      });
      await expectBarInView(page, "Continue");
      await expect(bar(page)).toHaveAttribute("data-more", "false");
    });
  });
}

test.describe("a disabled Continue", () => {
  test("shows its reason beside it, in view, and ties it to the button", async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 600 });
    await open(page, CONNECT);
    const next = bar(page).getByRole("button", { name: "Continue" });
    await expect(next).toBeDisabled();
    const reason = bar(page).getByText("Continue works once the check passes.");
    await expect(reason).toBeVisible();
    await expect(next).toHaveAccessibleDescription("Continue works once the check passes.");
    const size = page.viewportSize();
    const box = await next.boundingBox();
    expect(box && size && box.y + box.height <= size.height).toBe(true);
  });
});

test.describe("focus is not obscured by the bar", () => {
  test("every field of the connect step comes clear of the bar when it takes focus", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 900, height: 480 });
    await open(page, CONNECT);
    const fields = [/^Address/, /^Key/, /^Which one to use/];
    for (const label of fields) {
      await page.keyboard.press("Tab");
      const field = page.getByLabel(label);
      await field.focus();
      await expect(field).toBeFocused();
      const [box, frame] = await Promise.all([field.boundingBox(), bar(page).boundingBox()]);
      expect(box && frame).toBeTruthy();
      if (!box || !frame) return;
      expect(box.y + box.height, String(label)).toBeLessThanOrEqual(frame.y + 1);
    }
  });

  test("a field that sits under the bar is scrolled clear of it when it takes focus", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 900, height: 800 });
    await open(page, ONLINE);
    await page.locator("section h1").evaluate((h1) => {
      const spacer = document.createElement("div");
      const probe = document.createElement("input");
      probe.setAttribute("aria-label", "Probe");
      h1.parentElement?.append(spacer, probe);
      const barTop = document.querySelector("[data-action-bar]")?.getBoundingClientRect().top ?? 0;
      spacer.style.height = "1px";
      const start = probe.getBoundingClientRect().top;
      spacer.style.height = `${1 + barTop + 6 - start}px`;
    });
    const probe = page.getByLabel("Probe");
    const [before, frame] = await Promise.all([probe.boundingBox(), bar(page).boundingBox()]);
    expect(before && frame).toBeTruthy();
    if (!before || !frame) return;
    expect(before.y + before.height).toBeGreaterThan(frame.y);
    await probe.focus();
    await expect(probe).toBeFocused();
    const [box, now] = await Promise.all([probe.boundingBox(), bar(page).boundingBox()]);
    expect(box && now).toBeTruthy();
    if (!box || !now) return;
    expect(box.y + box.height).toBeLessThanOrEqual(now.y + 1);
  });

  test("the scrolling area keeps room as tall as the bar", async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 480 });
    await open(page, CONNECT);
    const padding = await (await scroller(page)).evaluate(
      (el) => getComputedStyle(el as Element).scrollPaddingBottom,
    );
    const frame = await bar(page).boundingBox();
    expect(Number.parseFloat(padding)).toBeCloseTo(frame?.height ?? -1, 0);
  });

  test("a very short window leaves the bar after the words", async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 400 });
    await open(page, CONNECT);
    expect(await bar(page).evaluate((el) => getComputedStyle(el).position)).toBe("static");
  });
});

test.describe("below 720 pixels wide", () => {
  test.use({ viewport: { width: 480, height: 720 } });

  test("the keep or ask bar is hidden during a step flow and back after it", async ({ page }) => {
    await open(page, ONLINE);
    const keep = page.getByRole("textbox", { name: "Keep something, or ask" });
    await expect(keep).toBeHidden();
    await expectBarInView(page, "Continue");
    await page.getByRole("button", { name: "Not now" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Online answers" })).toBeVisible();
    await expect(keep).toBeVisible();
  });

  test("the keep or ask bar stays on a screen that is not a flow", async ({ page }) => {
    await open(page, "/#/settings");
    await expect(page.getByRole("textbox", { name: "Keep something, or ask" })).toBeVisible();
  });
});

test.describe("the side column", () => {
  test("drops under the words below 1024 pixels and stays beside them above", async ({ page }) => {
    for (const [width, below] of [
      [900, true],
      [1280, false],
    ] as const) {
      await page.setViewportSize({ width, height: 800 });
      await open(page, "/?mockOnline=off&mockCheckMs=150#/settings/online/setup?step=send");
      const aside = page.locator("aside").first();
      const main = aside.locator("xpath=preceding-sibling::section");
      // The layout settles after the resize and the route change, so ask until it has.
      await expect
        .poll(
          async () => {
            const [a, m] = await Promise.all([aside.boundingBox(), main.boundingBox()]);
            return a && m ? a.y >= m.y + m.height - 1 : null;
          },
          { message: `${width}` },
        )
        .toBe(below);
    }
  });
});
