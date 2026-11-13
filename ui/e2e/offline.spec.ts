import { expect, test } from "@playwright/test";

const ROUTES = [
  "#/today",
  "#/pages/2026-11-12",
  "#/ask",
  "#/needs-you",
  "#/recent",
  "#/quick-capture",
  "#/memories",
  "#/memories/ent-leo",
  "#/items/item-letter",
  "#/fading",
  "#/pages",
  "#/setup",
  "#/privacy",
  "#/settings",
  "#/settings/advanced",
  "#/listen/2026-11-13",
  "#/print/2026-11-13",
];

test("every route loads with nothing leaving the preview origin and no policy violation", async ({
  page,
  baseURL,
}) => {
  const origin = new URL(baseURL ?? "http://localhost:4174").origin;
  const foreign: string[] = [];
  const violations: string[] = [];
  const consoleErrors: string[] = [];

  await page.route("**/*", (route) => {
    const url = route.request().url();
    if (url.startsWith("data:") || url.startsWith("blob:") || new URL(url).origin === origin) {
      return route.continue();
    }
    foreign.push(url);
    return route.abort();
  });
  await page.exposeFunction("reportViolation", (detail: string) => violations.push(detail));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      (window as unknown as { reportViolation: (d: string) => void }).reportViolation(
        `${event.violatedDirective} ${event.blockedURI}`,
      );
    });
  });
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  for (const route of ROUTES) {
    await page.goto(`/${route}`);
    await page.getByRole("heading", { level: 1 }).first().waitFor();
    await page.evaluate(() => document.fonts.ready);
  }

  expect(foreign).toEqual([]);
  expect(violations).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("the production policy header is sent", async ({ page }) => {
  const response = await page.goto("/");
  const csp = response?.headers()["content-security-policy"] ?? "";
  expect(csp).toContain("default-src 'self'");
  expect(csp).toContain("font-src 'self'");
  expect(csp).toContain("connect-src 'self' ipc: http://ipc.localhost");
});

test("fonts load from the app itself", async ({ page }) => {
  await page.goto("/#/today");
  await page.getByRole("heading", { level: 1 }).waitFor();
  const faces = await page.evaluate(async () => {
    await document.fonts.ready;
    return [...document.fonts]
      .filter((f) => f.status === "loaded")
      .map((f) => f.family.replace(/"/g, ""));
  });
  expect(faces.length).toBeGreaterThan(0);
  expect(new Set(faces).size).toBeGreaterThan(0);
});
