import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { gzipSync } from "node:zlib";
import { test as base, expect, type Page } from "@playwright/test";
import { build, preview } from "vite";
import { runAxe } from "./axe";

// The phone app at its own viewport, built with the scripted engine (no computer, no network)
// and served by Vite's preview with the phone's own policy, so anything the real page would
// block fails here too.

const POLICY = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self'",
  "img-src 'self' blob:",
  "media-src blob:",
  "font-src 'self'",
  "connect-src 'self' https://*.trycloudflare.com wss://*.trycloudflare.com",
  "worker-src 'self'",
  "manifest-src 'self'",
  "frame-ancestors 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

// biome-ignore lint/suspicious/noExplicitAny: in-page modules are reached through untyped globals
type Loose = any;

const root = resolve(process.cwd());

async function bundle(mode: "test" | "production", outDir: string) {
  await build({
    configFile: resolve(root, "vite.phone.config.ts"),
    mode,
    logLevel: "silent",
    build: { outDir, emptyOutDir: true },
  });
}

const test = base.extend<object, { phoneUrl: string }>({
  phoneUrl: [
    async ({ browserName }, use, workerInfo) => {
      const out = resolve(root, "test-results", `phone-${browserName}-${workerInfo.workerIndex}`);
      await bundle("test", out);
      const server = await preview({
        root: resolve(root, "phone"),
        configFile: false,
        build: { outDir: out },
        appType: "spa",
        logLevel: "silent",
        preview: {
          port: Number(process.env.PW_PORT ?? 4174) + 100 + workerInfo.workerIndex,
          strictPort: false,
          headers: {
            "Content-Security-Policy": POLICY,
            "Referrer-Policy": "no-referrer",
            "X-Content-Type-Options": "nosniff",
          },
        },
      });
      await use(server.resolvedUrls?.local[0]?.replace(/\/$/, "") ?? "");
      await new Promise<void>((done) => server.httpServer.close(() => done()));
    },
    { scope: "worker", timeout: 120_000 },
  ],
});

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, deviceScaleFactor: 2 });

interface Watch {
  violations: string[];
  foreign: string[];
  errors: string[];
}

async function watch(page: Page, origin: string): Promise<Watch> {
  const seen: Watch = { violations: [], foreign: [], errors: [] };
  await page.route("**/*", (route) => {
    const url = route.request().url();
    if (url.startsWith("data:") || url.startsWith("blob:") || new URL(url).origin === origin) {
      return route.continue();
    }
    seen.foreign.push(url);
    return route.abort();
  });
  await page.exposeFunction("reportViolation", (d: string) => seen.violations.push(d));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) => {
      (window as unknown as { reportViolation(d: string): void }).reportViolation(
        `${e.violatedDirective} ${e.blockedURI}`,
      );
    });
  });
  page.on("console", (m) => {
    if (m.type() === "error") seen.errors.push(m.text());
  });
  page.on("pageerror", (e) => seen.errors.push(String(e)));
  return seen;
}

const SCENARIOS: Array<[string, string]> = [
  ["connected", "Connected to Home computer"],
  ["connecting", "Connecting to Home computer…"],
  ["asleep", "Home computer is asleep, switched off or has a new address."],
  ["asleepMostPrivate", "If you use Most private, check Tailscale is on."],
  ["cantConnect", "Can’t connect to Home computer right now."],
  ["turnedOff", "Away from home is off."],
  ["outdated", "This phone needs adding again."],
  ["removed&waiting=2", "This phone was removed from Nocturne."],
  ["welcome", "Add this phone from your computer"],
  ["welcomeWaiting", "Add this phone from your computer"],
  ["typed", "Type the code from your computer"],
  ["pair", "Name this phone"],
  ["iphone", "Add Nocturne to your Home Screen"],
  ["tooOld", "This browser is too old"],
  ["relinkHere", "Open Nocturne from your home screen"],
  ["journalAsleep", "Waiting on this phone"],
  ["journalEmpty", "Nothing kept today yet."],
  ["checkIn", "Tap or drag to a spot, then pick the words that fit."],
  ["lookBackDay", "Wednesday 18 November · Looked back on"],
  ["lookBackWeek", "9 to 15 November"],
  ["lookBackToday", "Thursday 19 November"],
  ["whichGone", "Nothing to answer here now."],
  ["pageQuestion", "One question · from your journal"],
  ["journalAnswer", "About: “Parents’ evening at six. Not looking forward to it.”"],
];

async function axeClean(page: Page) {
  const result = await runAxe(page);
  expect(
    result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`),
  ).toEqual([]);
}

for (const scheme of ["light", "dark"] as const) {
  test(`every screen at 390 x 844 is accessible, in the policy, and fits the width (${scheme})`, async ({
    page,
    phoneUrl,
  }) => {
    // Twenty-four screens, each with an accessibility scan: slow on a shared machine.
    test.setTimeout(165_000);
    const seen = await watch(page, new URL(phoneUrl).origin);
    await page.emulateMedia({ colorScheme: scheme });
    for (const [scenario, text] of SCENARIOS) {
      await page.goto(`${phoneUrl}/?s=${scenario}`);
      await expect(page.getByText(text).first(), scenario).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await axeClean(page);
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow, `${scenario} scrolls sideways`).toBeLessThanOrEqual(0);
    }
    expect(seen.foreign).toEqual([]);
    expect(seen.violations).toEqual([]);
    expect(seen.errors).toEqual([]);
  });
}

test("pairing: name, check the words, allowed, then home", async ({ page, phoneUrl }) => {
  const seen = await watch(page, new URL(phoneUrl).origin);
  await page.goto(`${phoneUrl}/?s=pair`);
  await expect(page.getByRole("heading", { name: "Name this phone" })).toBeFocused();
  await expect(page.getByLabel("Name")).toHaveValue("My phone");
  await page.getByLabel("Name").fill("Pixel");
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Check your computer shows the same three words." }),
  ).toBeVisible();
  await expect(
    page.getByRole("list", { name: "Check words: harbour, maple, lantern" }),
  ).toBeVisible();
  await axeClean(page);
  await expect(page.getByRole("heading", { name: "Pixel can now use Nocturne." })).toBeVisible();
  await page.getByRole("button", { name: "Not now, open Nocturne" }).click();
  await expect(page.getByText("Connected to Home computer")).toBeVisible();
  expect(seen.violations).toEqual([]);
});

test("pairing: a refusal, a spent code and an expired code each say to start again", async ({
  page,
  phoneUrl,
}) => {
  for (const [scenario, title] of [
    ["pairRefused", "Your computer said no."],
    ["pairExpired", "This code has run out."],
    ["pairSpent", "This code can’t be used again."],
  ]) {
    await page.goto(`${phoneUrl}/?s=${scenario}`);
    await page.getByRole("button", { name: "Connect" }).click();
    await expect(page.getByRole("heading", { name: title as string })).toBeVisible();
    await axeClean(page);
    await page.getByRole("button", { name: "Start again" }).click();
    await expect(page.getByText("Add this phone from your computer")).toBeVisible();
  }
});

test("the typed code is four groups, normalised, and a paste fills them all", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=typed`);
  const continueButton = page.getByRole("button", { name: "Continue" });
  await expect(continueButton).toHaveAttribute("aria-disabled", "true");
  await page.getByLabel("Group 1").focus();
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData("text", "vj27f8 n0w53w-8b8zrb zcgsjn");
    document.activeElement?.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  });
  await expect(page.getByLabel("Group 1")).toHaveValue("VJ27F8");
  await expect(page.getByLabel("Group 4")).toHaveValue("ZCGSJN");
  await expect(continueButton).not.toHaveAttribute("aria-disabled", "true");
  await continueButton.click();
  await expect(page.getByRole("heading", { name: "Name this phone" })).toBeVisible();
});

test("home: three actions when connected; Ask and Today’s page off when the computer sleeps", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  for (const name of [/^Ask/, /^Keep/, /^Today’s page/]) {
    await expect(page.getByRole("button", { name })).toBeVisible();
  }
  await expect(page.getByText("Two things need you today.")).toBeVisible();
  await page.goto(`${phoneUrl}/?s=asleep`);
  await expect(page.getByRole("button", { name: /^Ask/ })).toHaveAttribute("aria-disabled", "true");
  await expect(page.getByRole("button", { name: /^Today’s page/ })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await expect(page.getByRole("button", { name: /^Keep/ })).toContainText("2 waiting");
  await expect(page.getByText("Last connected 07:48 · tries again by itself")).toBeVisible();
});

test("settings sheet: details, then a confirmed disconnect forgets the phone", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.getByRole("button", { name: "Menu" }).click();
  const dialog = page.getByRole("dialog", { name: "This phone" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Simple, through Cloudflare")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Done" })).toBeFocused();
  await axeClean(page);
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("button", { name: "Disconnect this phone…" }).click();
  await expect(page.getByRole("heading", { name: "Disconnect this phone?" })).toBeVisible();
  await page.getByRole("button", { name: "Stay connected" }).click();
  await expect(page.getByRole("heading", { name: "This phone" })).toBeVisible();
  await page.getByRole("button", { name: "Disconnect this phone…" }).click();
  await page.getByRole("button", { name: "Disconnect", exact: true }).click();
  await expect(page.getByText("Add this phone from your computer")).toBeVisible();
});

test("keep: a note is kept, then taken back", async ({ page, phoneUrl }) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.getByRole("button", { name: /^Keep/ }).click();
  const keep = page.getByRole("button", { name: "Keep", exact: true });
  await expect(keep).toHaveAttribute("aria-disabled", "true");
  await page.getByLabel("Type something to keep").fill("Swimming is Mondays at 17:30 now");
  await axeClean(page);
  await keep.click();
  await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
  await expect(page.getByText("kept from Pixel")).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: "Take back" }).click();
  await expect(page.getByText("Taken back. It’s no longer kept.")).toBeVisible();
});

const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

test("keep: a photo with a note capped at 120 characters", async ({ page, phoneUrl }) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({ name: "p.png", mimeType: "image/png", buffer: PNG });
  await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
  await expect(page.getByRole("img", { name: "The photo you are about to keep" })).toBeVisible();
  await page.getByLabel(/Add a note/).fill("x".repeat(150));
  await expect(page.getByLabel(/Add a note/)).toHaveValue("x".repeat(120));
  await axeClean(page);
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
});

test("keep: when the computer is asleep, a note waits on the phone and is counted", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=asleep`);
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page.getByLabel("Type something to keep").fill("Bin day moved to Thursday");
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Waiting." })).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: "Home", exact: true }).click();
  await expect(page.getByRole("button", { name: /^Keep/ })).toContainText("3 waiting");
});

test("ask: streams, shows where it came from, opens the original and comes back to the answer", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected&pace=250`);
  await page.getByRole("button", { name: /^Ask/ }).click();
  await axeClean(page);
  await page.getByLabel("Your question").fill("What do I need for Leo’s school trip?");
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
  await expect(page.getByText("Where this came from shows when the answer ends.")).toBeVisible();
  await expect(page.getByText("From 2 things you kept")).toBeVisible();
  await expect(page.getByText("Something wrong? Correct it on your computer.")).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: /Year 4 trip letter/ }).click();
  await expect(page.getByRole("heading", { name: "Year 4 trip letter" })).toBeVisible();
  await expect(page.getByText("nothing is saved on this phone")).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: "Answer" }).click();
  await expect(page.getByText("From 2 things you kept")).toBeVisible();
});

test("ask: Online answers' notice waits on the phone, and Continue sends the question", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected&onlineChanged=1`);
  await page.getByRole("button", { name: /^Ask/ }).click();
  await page.getByLabel("Your question").fill("What do I need for Leo’s school trip?");
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Online answers has changed" })).toBeFocused();
  await expect(page.getByText("Your question waits until you continue.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Answer on your computer" })).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Answered online by your provider (api.example.test)")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Online answers has changed" })).toHaveCount(0);
});

test("ask: not sure shows both, nothing and too many say so, stop cancels", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.getByRole("button", { name: /^Ask/ }).click();
  const ask = async (q: string) => {
    await page.getByLabel("Your question").fill(q);
    await page.getByRole("button", { name: "Ask", exact: true }).click();
  };
  await ask("When is Leo’s dentist appointment?");
  await expect(page.getByText("Tuesday 18 Nov, 14:00")).toBeVisible();
  await expect(page.getByText("Thursday 20 Nov, 10:00")).toBeVisible();
  await expect(
    page.getByText("Choose which is right on your computer and Nocturne will remember."),
  ).toBeVisible();
  await axeClean(page);
  await ask("nothing here");
  await expect(page.getByText("Nothing you’ve kept answers that.")).toBeVisible();
  await ask("too many");
  await expect(page.getByText("That’s a lot of questions at once.")).toBeVisible();
});

for (const scheme of ["light", "dark"] as const) {
  test(`ask: the online mark is accessible (${scheme})`, async ({ page, phoneUrl }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(`${phoneUrl}/?s=connected&pace=20`);
    await page.getByRole("button", { name: /^Ask/ }).click();
    await page.getByLabel("Your question").fill("What do I need for the trip, online?");
    await page.getByRole("button", { name: "Ask", exact: true }).click();
    await expect(
      page.getByText("Answered online by your provider (api.example.test)"),
    ).toBeVisible();
    await expect(
      page.getByText(
        "What was sent is listed on your computer. Choose Privacy, then See what was sent.",
      ),
    ).toBeVisible();
    await axeClean(page);
  });
}

test("ask: Stop ends the answer where it is", async ({ page, phoneUrl }) => {
  await page.goto(`${phoneUrl}/?s=connected&pace=600`);
  await page.getByRole("button", { name: /^Ask/ }).click();
  await page.getByLabel("Your question").fill("What do I need for Leo’s school trip?");
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await page.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByText("Stopped.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Stop" })).toBeHidden();
});

test("today’s page: the phone edition, earlier days, and an original from a source", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.getByRole("button", { name: /^Today’s page/ }).click();
  await expect(page.getByRole("heading", { name: "Two things need you today." })).toBeVisible();
  await expect(page.getByText("Phone edition · private lines left out")).toBeVisible();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Earlier pages" }).getByRole("button"),
  ).toHaveCount(3);
  await axeClean(page);
  await page
    .getByRole("navigation", { name: "Earlier pages" })
    .getByRole("button", { name: /Thu 12/ })
    .click();
  await expect(page.getByRole("heading", { name: "A quiet Wednesday." })).toBeVisible();
  await page.getByRole("navigation", { name: "Earlier pages" }).getByRole("button").first().click();
  await page
    .getByRole("button", { name: /Message you pasted/ })
    .first()
    .click();
  await expect(
    page.getByText("Packed lunch in a bag they can throw away please!").first(),
  ).toBeVisible();
  await expect(page.locator("mark")).toContainText("Packed lunch");
  await axeClean(page);
});

test("today’s page: the question from the journal, answered with a line", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=pageQuestion`);
  const box = page.getByRole("region", { name: /Yesterday you wrote/ });
  await expect(box).toContainText("“Parents’ evening at six. Not looking forward to it.”");
  await axeClean(page);
  await box.getByRole("button", { name: "Not great" }).click();
  await expect(box.getByText("Not great. Kept on today’s journal.")).toBeVisible();
  const field = box.getByLabel("Anything to add? (optional)");
  await expect(field).toBeFocused();
  await axeClean(page);
  await field.fill("Mrs Okafor is worried about his reading.");
  await box.getByRole("button", { name: "Add" }).click();
  await expect(box.getByText("“Mrs Okafor is worried about his reading.” Added.")).toBeVisible();
  await axeClean(page);
  const width = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(width).toBeLessThanOrEqual(0);
  await box.getByRole("button", { name: "Open today’s journal ›" }).click();
  await expect(page.getByRole("heading", { name: "Journal" })).toBeVisible();
});

test("today’s page: the question answered first elsewhere, waiting, and refused", async ({
  page,
  phoneUrl,
}) => {
  for (const [scenario, choice, text] of [
    ["pageElsewhere", "Not great", "Already answered, on your computer or another phone."],
    ["pageWaiting", "Not great", "Not great. Waiting on this phone."],
    ["pageRefused", "Fine", "That didn’t go across."],
  ] as const) {
    await page.goto(`${phoneUrl}/?s=${scenario}`);
    const box = page.getByRole("region", { name: /Yesterday you wrote/ });
    await box.getByRole("button", { name: choice }).click();
    await expect(box.getByText(text), scenario).toBeVisible();
    await axeClean(page);
  }
});

test("today’s page: none yet says so", async ({ page, phoneUrl }) => {
  await page.goto(`${phoneUrl}/?s=noPage`);
  await page.getByRole("button", { name: /^Today’s page/ }).click();
  await expect(page.getByRole("heading", { name: "No page yet." })).toBeVisible();
  await axeClean(page);
});

test("a voice note original plays from a blob and the blob is released on leaving", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await page.evaluate(() => {
    const w = window as unknown as { revoked: string[]; made: string[] };
    w.revoked = [];
    w.made = [];
    const make = URL.createObjectURL.bind(URL);
    const revoke = URL.revokeObjectURL.bind(URL);
    URL.createObjectURL = (b: Blob | MediaSource) => {
      const u = make(b);
      w.made.push(u);
      return u;
    };
    URL.revokeObjectURL = (u: string) => {
      w.revoked.push(u);
      revoke(u);
    };
  });
  await page.getByRole("button", { name: /^Today’s page/ }).click();
  await page
    .getByRole("button", { name: /Voice note · 0:08/ })
    .first()
    .click();
  await expect(page.getByRole("button", { name: "Play voice note" })).toBeVisible();
  await expect(page.getByText("What Nocturne heard")).toBeVisible();
  await axeClean(page);
  await page.getByRole("button", { name: "Answer" }).click();
  const state = await page.evaluate(() => {
    const w = window as unknown as { revoked: string[]; made: string[] };
    return { made: w.made, revoked: w.revoked };
  });
  expect(state.made.length).toBeGreaterThan(0);
  for (const u of state.made) expect(state.revoked).toContain(u);
});

test("the app can be driven from the keyboard alone", async ({ page, phoneUrl }) => {
  await page.goto(`${phoneUrl}/?s=connected`);
  await expect(page.getByText("Connected to Home computer")).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Menu" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: /^Ask/ })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("heading", { name: "Ask about anything you’ve kept." }),
  ).toBeVisible();
});

test("keys.survive_reload: the device key is stored in the browser, cannot be read out, and outlives a reload", async ({
  page,
  phoneUrl,
}) => {
  await page.goto(`${phoneUrl}/?s=welcome`);
  await page.waitForFunction(
    () => (window as unknown as { __phone?: object }).__phone !== undefined,
  );
  const saved = await page.evaluate(async () => {
    const { crypto: c, keys } = (window as unknown as { __phone: { crypto: Loose; keys: Loose } })
      .__phone;
    const d = await c.generateDeviceKey();
    const other = await c.generateDeviceKey();
    const store = keys.idbKeyStore();
    await store.clear();
    await store.save({
      deviceId: new Uint8Array(16).fill(7),
      privateKey: d.privateKey,
      publicKey: d.publicKey,
      cpub: other.publicKey,
      homeOrigin: location.origin,
      address: location.host,
      deviceName: "Pixel",
      computerName: null,
      connection: null,
      version: null,
      pairedAt: 1,
      lastConnectedAt: null,
    });
    return true;
  });
  expect(saved).toBe(true);
  await page.reload();
  await page.waitForFunction(
    () => (window as unknown as { __phone?: object }).__phone !== undefined,
  );
  const after = await page.evaluate(async () => {
    const { keys } = (window as unknown as { __phone: { keys: Loose } }).__phone;
    const device = await keys.idbKeyStore().device();
    let exportable = true;
    try {
      await crypto.subtle.exportKey("pkcs8", device.privateKey);
    } catch {
      exportable = false;
    }
    const local = Object.keys(localStorage).length + Object.keys(sessionStorage).length;
    await keys.idbKeyStore().clear();
    return {
      name: device.deviceName,
      extractable: device.privateKey.extractable,
      exportable,
      local,
    };
  });
  expect(after).toEqual({ name: "Pixel", extractable: false, exportable: false, local: 0 });
});

test("WebCrypto in the browser reproduces the reference vectors", async ({ page, phoneUrl }) => {
  const ref = JSON.parse(
    readFileSync(resolve(root, "../src-tauri/tests/fixtures/remote-vectors.json"), "utf8"),
  );
  await page.goto(`${phoneUrl}/?s=welcome`);
  await page.waitForFunction(
    () => (window as unknown as { __phone?: object }).__phone !== undefined,
  );
  const result = await page.evaluate(async (vectors) => {
    const { crypto: c } = (window as unknown as { __phone: { crypto: Loose } }).__phone;
    const hex = (s: string) => Uint8Array.from(s.match(/../g) ?? [], (h) => Number.parseInt(h, 16));
    const toHex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
    const pos = (n: string) => vectors.positive.find((v: Loose) => v.name === n);
    const out: string[] = [];
    const check = (name: string, got: string, want: string) => {
      if (got !== want) out.push(`${name}: ${got} != ${want}`);
    };

    const k = pos("pair_keys");
    const keys = await c.pairKeys(hex(k.inputs.s), hex(k.inputs.cpub), hex(k.inputs.pairingId));
    check("pair up", toHex(keys.up), k.output.up);
    check("pair down", toHex(keys.down), k.output.down);

    const sk = pos("session_keys");
    const terms = {
      deviceId: hex(sk.inputs.deviceId),
      epub: hex(sk.inputs.epub),
      openNonce: hex(sk.inputs.openNonce),
      sid: hex(sk.inputs.sid),
      fpub: hex(sk.inputs.fpub),
      cpub: hex(sk.inputs.cpub),
      dpub: hex(sk.inputs.dpub),
    };
    const ikm = ["ecdhEF", "ecdhEC", "ecdhDF", "ecdhDC"].map((n) => hex(sk.intermediate[n]));
    const sess = await c.sessionKeyBytes(terms, ikm);
    check("session up", toHex(sess.up), sk.output.up);
    check("session down", toHex(sess.down), sk.output.down);

    for (const v of vectors.positive.filter((x: Loose) => x.step === "data")) {
      const env = hex(v.intermediate.envelope ?? v.inputs.envelope);
      const frame = await c.sealDataFrame(
        hex(v.inputs.key),
        v.inputs.direction,
        hex(v.inputs.sid),
        v.inputs.ctr,
        env,
      );
      check(v.name, toHex(frame), v.output.frame);
    }
    for (const v of vectors.positive.filter((x: Loose) => x.step === "pairFrame")) {
      const frame = await c.sealPairFrame(
        v.inputs.kind,
        hex(v.inputs.pairingId),
        hex(v.inputs.key),
        hex(v.inputs.nonce),
        hex(v.intermediate.envelope),
      );
      check(v.name, toHex(frame), v.output.frame);
    }
    for (const v of vectors.negative.filter((x: Loose) => x.step === "publicKey")) {
      try {
        await c.importPublicKey(hex(v.inputs.key));
        out.push(`${v.name} accepted`);
      } catch (e) {
        check(v.name, (e as Loose).code, v.expect);
      }
    }
    for (const v of vectors.negative.filter((x: Loose) => x.step === "data")) {
      try {
        await c.openDataFrame(
          hex(v.inputs.frame),
          hex(v.inputs.key),
          v.inputs.direction,
          hex(v.inputs.sid),
          v.inputs.lastCtr,
        );
        out.push(`${v.name} accepted`);
      } catch (e) {
        check(v.name, (e as Loose).code, v.expect);
      }
    }
    return out;
  }, ref);
  expect(result).toEqual([]);
});

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = resolve(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

test("bundle.under_budget: the shipped phone app is under 300 KB gzip of script and style, and carries no test code", async ({
  browserName,
}, workerInfo) => {
  test.skip(browserName !== "chromium", "the build does not depend on the browser");
  const out = resolve(root, "test-results", `phone-production-${workerInfo.workerIndex}`);
  await bundle("production", out);
  const all = files(out);
  const code = all.filter((f) => /\.(js|css)$/.test(f));
  const gzipped = code.reduce((n, f) => n + gzipSync(readFileSync(f)).length, 0);
  expect(gzipped).toBeLessThan(300 * 1024);
  const text = code.map((f) => readFileSync(f, "utf8")).join("\n");
  for (const forbidden of [
    "FakeComputer",
    "createMockPhoneEngine",
    "mockServices",
    "Waiting note",
  ]) {
    expect(text.includes(forbidden), forbidden).toBe(false);
  }
  expect(all.some((f) => f.endsWith("/sw.js"))).toBe(true);
  const html = readFileSync(resolve(out, "index.html"), "utf8");
  expect(html).not.toMatch(/<script(?![^>]*\bsrc=)[^>]*>/);
  expect(html).not.toMatch(/\sstyle=|<style/);
  void createHash;
});
