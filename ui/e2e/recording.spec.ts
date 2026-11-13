import { expect, test } from "@playwright/test";

// Chromium's fake microphone, so the web preview records a real sound without a person.
test.use({
  launchOptions: {
    args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  },
  permissions: ["microphone"],
});

test("hold to talk keeps a voice note that shows in Recent and plays on its own page", async ({
  page,
}) => {
  await page.goto("/#/today");
  const talk = page.getByRole("button", { name: "Hold to talk" });
  await expect(talk).not.toHaveAttribute("aria-disabled", "true");

  const box = await talk.boundingBox();
  if (!box) throw new Error("the microphone button is not on screen");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await expect(page.getByText(/^Listening/)).toBeVisible();
  await page.waitForTimeout(1500);
  await page.mouse.up();
  await expect(page.getByText(/Voice note, 0:0\d\./)).toBeVisible();

  await page.goto("/#/recent");
  const row = page
    .getByRole("list", { name: "Things you kept today and yesterday" })
    .locator("li", { hasText: "Voice note" })
    .first();
  await expect(row).toContainText("Kept · reads tonight");
  await expect(row.getByRole("button", { name: "Read now: Voice note" })).toBeVisible();

  // The first thing kept in a fresh preview is new-1.
  await page.goto("/#/items/new-1");
  await expect(page.getByText(/Understood in/)).toHaveCount(0);
  const audio = page.locator("audio");
  await expect(audio).toHaveCount(1);
  await expect
    .poll(() => audio.evaluate((el: HTMLAudioElement) => el.readyState))
    .toBeGreaterThan(0);

  // The kept file is a 16 kHz mono 16-bit WAV, which the speech model can read.
  const wav = await audio.evaluate(async (el: HTMLAudioElement) => {
    const buffer = await (await fetch(el.src)).arrayBuffer();
    const view = new DataView(buffer);
    const word = (at: number) => String.fromCharCode(...new Uint8Array(buffer, at, 4));
    return {
      riff: word(0),
      wave: word(8),
      channels: view.getUint16(22, true),
      rate: view.getUint32(24, true),
      bits: view.getUint16(34, true),
      declared: view.getUint32(40, true),
      size: buffer.byteLength,
    };
  });
  expect(wav).toMatchObject({ riff: "RIFF", wave: "WAVE", channels: 1, rate: 16000, bits: 16 });
  expect(wav.declared).toBe(wav.size - 44);
  expect(wav.size).toBeGreaterThan(32_000);

  // The waveform is drawn from the sound itself, not from even bars.
  await expect(page.locator('[data-measured="true"]')).toHaveCount(1);

  await page.getByRole("button", { name: "Play" }).click();
  await expect
    .poll(() => audio.evaluate((el: HTMLAudioElement) => el.currentTime), { timeout: 5000 })
    .toBeGreaterThan(0);
  await expect(page.getByText("Nocturne can’t play this here.")).toHaveCount(0);

  const opened = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Open original" }).click();
  expect((await opened).url()).toMatch(/^blob:/);
});

test("a page that cannot record says so once and the button stays marked", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "mediaDevices", { value: undefined, configurable: true });
  });
  await page.goto("/#/today");
  const talk = page.getByRole("button", { name: "Hold to talk" });
  await expect(talk).toHaveAttribute("aria-disabled", "true");
  for (let i = 0; i < 3; i += 1) await talk.click({ force: true });
  await expect(page.locator("output").getByText(/can’t record sound/)).toHaveCount(1);
});
