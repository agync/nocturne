// I2: the real app (mock runtime, scripted mind, stand-in helper that records what it relays) and
// the built phone app in Chromium at 390 x 844 with real WebCrypto. The whole session, once
// with a scanned code and once with a typed one, over a WebSocket and over long-poll.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { type How, instrument, type Mode, netEvents, pairPhone, photo } from "./phone";
import type { Computer } from "./world";

const KEPT_TEXT = "Parcel for PLAIN-TEXT-MARK-7d3f is behind the shed";
const PHOTO_MARK = "PHOTO-MARK-b81c";
const QUESTION = "When is the boiler serviced?";
const ANSWER = "The boiler is serviced every spring.";
const LATE_QUESTION = "When is the dentist?";
const TAKEN_BACK = "TAKE-BACK-MARK-02aa";
const NAME = "Pixel";

interface Item {
  id: string;
  kind: string;
  text: string | null;
  title: string;
  originalPath: string | null;
}

const items = async (computer: Computer) => (await computer.driver.call("items")) as Item[];

async function home(page: Page) {
  await page
    .getByRole("button", { name: /^(Back to home|Home)$/ })
    .first()
    .click();
  await expect(page.getByText(/^Connected to /)).toBeVisible();
}

/** Every file of the built phone app, as text, so a marker can be shown not to be in the app itself. */
function bundle(): string {
  const root = join(process.cwd(), "dist", "phone");
  const walk = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
      e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
    );
  return walk(root)
    .map((f) => readFileSync(f).toString("latin1"))
    .join("\n");
}

for (const [how, mode] of [
  ["scanned", "ws"],
  ["typed", "ws"],
  ["scanned", "poll"],
  ["typed", "poll"],
] as Array<[How, Mode]>) {
  test(`e2e_relay_sees_no_plaintext: ${how} code, ${mode === "ws" ? "WebSocket" : "long-poll"}`, async ({
    browser,
    computer,
  }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await instrument(page, mode);
    const d = computer.driver;
    const { offer } = await pairPhone(page, computer, how, NAME);

    // Keep a marked text, then a photo.
    await page.getByRole("button", { name: /^Keep/ }).click();
    await page.getByLabel("Type something to keep").fill(KEPT_TEXT);
    await page.getByRole("button", { name: "Keep", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
    expect(
      (await items(computer)).filter((i) => i.text?.includes("PLAIN-TEXT-MARK-7d3f")),
    ).toHaveLength(1);

    await page.getByRole("button", { name: "Keep another" }).click();
    await page
      .locator('input[type="file"]')
      .first()
      .setInputFiles({
        name: "shed.jpg",
        mimeType: "image/jpeg",
        buffer: photo(PHOTO_MARK, 700_000),
      });
    await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
    await page.getByRole("button", { name: "Keep", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
    const marked = (await items(computer)).filter(
      (i) => i.originalPath && readFileSync(i.originalPath).includes(PHOTO_MARK),
    );
    expect(marked, "the photo reached the computer once").toHaveLength(1);
    await home(page);

    // Ask, and see where the answer came from.
    await page.getByRole("button", { name: /^Ask/ }).click();
    await page.getByLabel("Your question").fill(QUESTION);
    await page.getByRole("button", { name: "Ask", exact: true }).click();
    await expect(page.getByText(ANSWER)).toBeVisible();
    await expect(page.getByText(/^From \d things? you kept/).first()).toBeVisible();
    await page.screenshot({ path: `test-results/away/${how}-${mode}-answer.png` });

    // Open the photo it came from, and come back to the answer.
    const sources = page
      .getByRole("list", { name: /^From \d things? you kept/ })
      .getByRole("button");
    await sources.first().click();
    await expect(page.getByRole("img").first()).toBeVisible();
    await page.screenshot({ path: `test-results/away/${how}-${mode}-original.png` });
    await page.getByRole("button", { name: "Answer" }).click();
    await expect(page.getByText(ANSWER)).toBeVisible();
    await home(page);

    // Read the page.
    await page.getByRole("button", { name: /^Today’s page/ }).click();
    await expect(page.getByText("Phone edition · private lines left out")).toBeVisible();
    await home(page);

    // Keep one more and take it back.
    await page.getByRole("button", { name: /^Keep/ }).click();
    await page.getByLabel("Type something to keep").fill(`Note ${TAKEN_BACK}`);
    await page.getByRole("button", { name: "Keep", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Kept." })).toBeVisible();
    await page.getByRole("button", { name: "Take back" }).click();
    await expect(page.getByText("Taken back. It’s no longer kept.")).toBeVisible();
    expect((await items(computer)).filter((i) => i.text?.includes(TAKEN_BACK))).toHaveLength(0);
    await home(page);

    // Remove the phone in the middle of an answer.
    const devices = (await d.call("devices")) as Array<{ id: string; name: string }>;
    expect(devices.map((x) => x.name)).toEqual([NAME]);
    await d.call("gate", { open: false });
    await page.getByRole("button", { name: /^Ask/ }).click();
    await page.getByLabel("Your question").fill(LATE_QUESTION);
    await page.getByRole("button", { name: "Ask", exact: true }).click();
    await d.call("entered");
    await expect.poll(() => d.call("inflight", { device: devices[0]?.id })).toBe(1);
    const removedAt = Date.now();
    await d.call("remove", { device: devices[0]?.id });
    let stoppedAfter = -1;
    while (Date.now() - removedAt < 5000) {
      if ((await d.call("inflight", { device: devices[0]?.id })) === 0) {
        stoppedAfter = Date.now() - removedAt;
        break;
      }
    }
    expect(stoppedAfter, "the answer stopped").toBeGreaterThanOrEqual(0);
    expect(stoppedAfter, "the answer stopped within a second").toBeLessThan(1000);
    await expect(page.getByText("This phone was removed from Nocturne.")).toBeVisible({
      timeout: 5000,
    });
    const shownAfter = Date.now() - removedAt;
    await d.call("gate", { open: true });
    const events = await netEvents(page);
    const ended = events.filter(
      (e) => e.kind === (mode === "ws" ? "ws-close" : "poll-end") && e.t >= removedAt,
    );
    expect(ended.length, "the held connection ended").toBeGreaterThan(0);
    const heldEnded = (ended[0]?.t ?? Infinity) - removedAt;
    expect(heldEnded, "the held connection ended within a second").toBeLessThan(1000);
    console.log(
      `timing ${how}/${mode}: answer stopped ${stoppedAfter} ms, held ${mode === "ws" ? "socket" : "poll"} ended ${heldEnded} ms, removed screen ${shownAfter} ms (after removal)`,
    );
    await page.screenshot({ path: `test-results/away/${how}-${mode}-removed.png` });

    // What the relay saw: bytes, and none of it readable.
    const seen = computer.relayed().toString("latin1");
    const app = bundle();
    expect(seen.length).toBeGreaterThan(500_000);
    // The recording reads what a relay would: the plain HTTP around the sealed bodies.
    expect(seen).toContain("GET /");
    expect(seen.toLowerCase()).toContain(mode === "ws" ? "upgrade: websocket" : "post /s ");
    if (mode === "poll") expect(seen.toLowerCase()).not.toContain("upgrade: websocket");
    const secrets = [
      "PLAIN-TEXT-MARK-7d3f",
      "PHOTO-MARK-b81c",
      "TAKE-BACK-MARK-02aa",
      KEPT_TEXT,
      QUESTION,
      LATE_QUESTION,
      ANSWER,
      "DESK-PHOTO-MARKER-5c1e",
      "dentist",
      "boiler",
      "Pixel",
      offer.typedCode,
      offer.typedCode.replace(/[^0-9A-Za-z]/g, ""),
      offer.link.split("#")[1] ?? "?",
    ];
    for (const s of secrets) {
      if (app.includes(s)) continue;
      expect(seen.includes(s), `the relay saw ${s}`).toBe(false);
    }
    expect(errors).toEqual([]);
    await context.close();
  });
}
