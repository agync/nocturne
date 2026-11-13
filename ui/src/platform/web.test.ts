import { afterEach, describe, expect, it, vi } from "vitest";
import { WebPlatform } from "./web";

function dragEvent(type: string, names: string[]): DragEvent {
  const event = new Event(type, { bubbles: true, cancelable: true }) as DragEvent;
  Object.defineProperty(event, "dataTransfer", {
    value: {
      types: ["Files"],
      items: names.map((n) => ({ kind: "file", name: n })),
      files: names.map((name) => ({ name })),
    },
  });
  return event;
}

describe("WebPlatform file drop", () => {
  const offs: Array<() => void> = [];
  afterEach(() => {
    for (const off of offs.splice(0)) off();
  });

  it("reports how many files are over the window, then the names on drop", () => {
    const onDrop = vi.fn();
    const onHover = vi.fn();
    offs.push(new WebPlatform().onFileDrop(onDrop, onHover));
    window.dispatchEvent(dragEvent("dragenter", ["a.jpg", "b.pdf"]));
    expect(onHover).toHaveBeenLastCalledWith(2);
    window.dispatchEvent(dragEvent("drop", ["a.jpg", "b.pdf"]));
    expect(onHover).toHaveBeenLastCalledWith(null);
    expect(onDrop).toHaveBeenCalledWith(["a.jpg", "b.pdf"]);
  });

  it("clears the hover when the files leave", () => {
    const onHover = vi.fn();
    offs.push(new WebPlatform().onFileDrop(vi.fn(), onHover));
    window.dispatchEvent(dragEvent("dragenter", ["a.jpg"]));
    window.dispatchEvent(dragEvent("dragleave", ["a.jpg"]));
    expect(onHover).toHaveBeenLastCalledWith(null);
  });

  it("ignores drags that are not files", () => {
    const onHover = vi.fn();
    offs.push(new WebPlatform().onFileDrop(vi.fn(), onHover));
    const event = new Event("dragenter", { cancelable: true }) as unknown as DragEvent;
    Object.defineProperty(event, "dataTransfer", { value: { types: ["text/plain"], items: [] } });
    window.dispatchEvent(event);
    expect(onHover).not.toHaveBeenCalled();
  });

  it("stops listening when unsubscribed", () => {
    const onHover = vi.fn();
    new WebPlatform().onFileDrop(vi.fn(), onHover)();
    window.dispatchEvent(dragEvent("dragenter", ["a.jpg"]));
    expect(onHover).not.toHaveBeenCalled();
  });
});
