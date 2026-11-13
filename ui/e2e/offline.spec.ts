import { expect, test } from "@playwright/test";

test("loads with no request leaving the preview origin", async ({ page, baseURL }) => {
  const origin = new URL(baseURL ?? "http://localhost:4174").origin;
  const foreign: string[] = [];
  await page.route("**/*", (route) => {
    const url = route.request().url();
    if (url.startsWith("data:") || url.startsWith("blob:") || new URL(url).origin === origin) {
      return route.continue();
    }
    foreign.push(url);
    return route.abort();
  });
  await page.goto("/");
  await page.evaluate(() => document.fonts.ready);
  expect(foreign).toEqual([]);
});
