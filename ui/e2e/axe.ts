import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";

/**
 * Runs axe on settled colours: contrast measured mid-transition is a false
 * failure, so motion is reduced and running animations finish first.
 */
export async function runAxe(
  page: Page,
  configure: (builder: AxeBuilder) => AxeBuilder = (b) => b,
) {
  await page.emulateMedia({ reducedMotion: "reduce" });
  try {
    await page.waitForFunction(
      () => document.getAnimations().every((a) => a.playState !== "running"),
      undefined,
      { timeout: 2000 },
    );
    return await configure(new AxeBuilder({ page })).analyze();
  } finally {
    await page.emulateMedia({ reducedMotion: null });
  }
}
