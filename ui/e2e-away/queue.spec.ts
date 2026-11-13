// I3: the computer goes away in the middle of an upload and comes back. Everything the phone kept
// meanwhile lands on the computer once, and only once.

import { readFileSync } from "node:fs";
import { expect, test } from "./fixtures";
import { instrument, pairPhone, photo } from "./phone";

const MARK = "SLEEP-PHOTO-MARK-91e0";

interface Item {
  id: string;
  text: string | null;
  originalPath: string | null;
}

test("e2e_queue_flushes_once: the listener stops mid-upload, two more keeps wait, each lands once", async ({
  browser,
  computer,
  front,
}) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  await instrument(page, "ws");
  const d = computer.driver;
  await pairPhone(page, computer, "scanned", "Pixel");
  const items = async () => (await d.call("items")) as Item[];

  // A large photo, slowed so the stop comes while it is on the wire.
  front.rate = 2_000_000;
  const before = computer.relayed().length;
  await page.getByRole("button", { name: /^Keep/ }).click();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({
      name: "big.jpg",
      mimeType: "image/jpeg",
      buffer: photo(MARK, 12_000_000),
    });
  await expect(page.getByRole("heading", { name: "Keep a photo" })).toBeVisible();
  await page.getByRole("button", { name: "Keep", exact: true }).click();
  await expect
    .poll(() => computer.relayed().length - before, { timeout: 30_000 })
    .toBeGreaterThan(2_000_000);
  expect(await d.call("off")).toBe("off");
  front.rate = 0;
  await expect(page.getByRole("heading", { name: "Waiting." })).toBeVisible({ timeout: 30_000 });
  expect(
    (await items()).filter((i) => i.originalPath && readFileSync(i.originalPath).includes(MARK)),
  ).toHaveLength(0);

  // Two more while it is away.
  for (const text of ["SLEEP-ONE-4c2a", "SLEEP-TWO-77be"]) {
    await page.getByRole("button", { name: "Keep another" }).click();
    await page.getByLabel("Type something to keep").fill(`Note ${text}`);
    await page.getByRole("button", { name: "Keep", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Waiting." })).toBeVisible();
  }
  expect((await items()).filter((i) => i.text?.includes("SLEEP-"))).toHaveLength(0);

  // Back: the phone is told it is online, as it is when it wakes.
  const back = (await d.call("on")) as { publicPort: number };
  front.target = back.publicPort;
  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await expect
    .poll(
      async () => {
        const all = await items();
        return [
          all.filter((i) => i.text?.includes("SLEEP-ONE-4c2a")).length,
          all.filter((i) => i.text?.includes("SLEEP-TWO-77be")).length,
          all.filter((i) => i.originalPath && readFileSync(i.originalPath).includes(MARK)).length,
        ];
      },
      { timeout: 60_000 },
    )
    .toEqual([1, 1, 1]);

  // Nothing is sent twice afterwards.
  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await page.waitForTimeout(3000);
  const all = await items();
  expect(all.filter((i) => i.text?.includes("SLEEP-ONE-4c2a"))).toHaveLength(1);
  expect(all.filter((i) => i.text?.includes("SLEEP-TWO-77be"))).toHaveLength(1);
  expect(
    all.filter((i) => i.originalPath && readFileSync(i.originalPath).includes(MARK)),
  ).toHaveLength(1);
  await context.close();
});
