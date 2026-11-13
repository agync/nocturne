import { describe, expect, it, vi } from "vitest";
import { HttpPlatform } from "./http";

describe("HttpPlatform", () => {
  it("shows an original from the server by link, with the media ticket", async () => {
    sessionStorage.setItem("nocturne.preview.token", "tok");
    const fetcher = vi.fn(async () => ({
      ok: true,
      json: async () => ({ ticket: "media1" }),
    })) as unknown as typeof fetch;
    const platform = await new HttpPlatform().start(fetcher);
    expect(platform.originalSrc("/originals/abc")).toBe("/originals/abc?t=media1");
    expect(platform.originalSrc("/audio/p.wav")).toBe("/audio/p.wav?t=media1");
    expect(platform.originalSrc("blob:local")).toBe("blob:local");
    expect(fetcher).toHaveBeenCalledWith(
      "/api/ticket",
      expect.objectContaining({
        headers: expect.objectContaining({ authorization: "Bearer tok" }),
      }),
    );
    sessionStorage.clear();
  });

  it("does nothing for what needs the desktop", async () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const platform = new HttpPlatform();
    expect(await platform.pickFolder()).toBeNull();
    await platform.openPath();
    await platform.openOriginal();
    await platform.openLogs();
    expect(info).toHaveBeenCalledTimes(3);
    info.mockRestore();
  });

  it("picks with a phone-friendly filter and keeps the input attached only while picking", async () => {
    const platform = new HttpPlatform();
    const click = vi.spyOn(HTMLInputElement.prototype, "click").mockImplementation(() => undefined);
    const picking = platform.pickFiles();
    const input = document.body.querySelector<HTMLInputElement>('input[type="file"]');
    expect(input).not.toBeNull();
    expect(input?.accept).toBe("image/*,application/pdf,audio/*,text/*");
    expect(input?.multiple).toBe(true);
    expect(input?.hidden).toBe(true);
    input?.dispatchEvent(new Event("cancel"));
    await expect(picking).resolves.toEqual([]);
    expect(document.body.querySelector('input[type="file"]')).toBeNull();
    click.mockRestore();
  });
});
