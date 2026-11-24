import { describe, expect, it } from "vitest";
import { followFeet, footPlace } from "./foot";

const screen = { viewport: 915, keyboard: false, scrolled: 0 };

describe("the step's bar on a phone", () => {
  it("pins however long the words, and fades them while more continue below", () => {
    expect(footPlace({ ...screen, foot: 160, height: 1800 })).toEqual({ pinned: true, more: true });
    expect(footPlace({ ...screen, foot: 160, height: 1800, scrolled: 885 })).toEqual({
      pinned: true,
      more: false,
    });
    expect(footPlace({ ...screen, foot: 160, height: 600 })).toEqual({ pinned: true, more: false });
  });

  it("pins up to a third of the screen, and sits after the words past it", () => {
    expect(footPlace({ ...screen, foot: 305, height: 1800 }).pinned).toBe(true);
    expect(footPlace({ ...screen, foot: 306, height: 1800 })).toEqual({
      pinned: false,
      more: false,
    });
    // A phone on its side.
    expect(footPlace({ ...screen, viewport: 412, foot: 160, height: 1800 }).pinned).toBe(false);
  });

  it("sits after the words while the keyboard is up", () => {
    expect(footPlace({ ...screen, keyboard: true, foot: 160, height: 1800 })).toEqual({
      pinned: false,
      more: false,
    });
  });

  it("marks each bar in the page and keeps room for it", () => {
    const root = document.createElement("div");
    root.innerHTML = '<main>Words</main><footer class="ph-foot">Continue</footer>';
    document.body.append(root);
    const stop = followFeet(root);
    const foot = root.querySelector<HTMLElement>(".ph-foot");
    expect(foot?.dataset.pinned).toBe("true");
    expect(foot?.dataset.more).toBe("false");
    document.documentElement.classList.add("ph-keyboard");
    stop();
    const again = followFeet(root);
    expect(foot?.dataset.pinned).toBe("false");
    again();
    document.documentElement.classList.remove("ph-keyboard");
    expect(document.documentElement.style.getPropertyValue("--ph-foot-room")).toBe("");
    root.remove();
  });
});
