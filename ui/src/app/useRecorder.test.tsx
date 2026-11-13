import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRecorder } from "./useRecorder";

class FakeRecorder {
  static last: FakeRecorder | null = null;
  state: "inactive" | "recording" = "inactive";
  mimeType = "audio/webm";
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  constructor(public stream: unknown) {
    FakeRecorder.last = this;
  }
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob([new Uint8Array([1, 2, 3, 4])]) });
    this.onstop?.();
  }
}

const track = { stop: vi.fn() };

beforeEach(() => {
  vi.stubGlobal("MediaRecorder", FakeRecorder);
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [track] }) },
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  track.stop.mockClear();
});

describe("useRecorder", () => {
  it("records while held and hands over the bytes when released", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    expect(result.current.seconds).toBeNull();
    act(() => result.current.start());
    await waitFor(() => expect(result.current.seconds).toBe(0));
    act(() => result.current.stop());
    await waitFor(() => expect(onFinished).toHaveBeenCalled());
    const [bytes, mime] = onFinished.mock.calls[0] as [Uint8Array, string, number];
    expect(Array.from(bytes)).toEqual([1, 2, 3, 4]);
    expect(mime).toBe("audio/webm");
    expect(result.current.seconds).toBeNull();
    expect(track.stop).toHaveBeenCalled();
  });

  it("says so when the microphone is turned off", async () => {
    (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mockRejectedValue(
      new DOMException("denied", "NotAllowedError"),
    );
    const onFailed = vi.fn();
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed }));
    act(() => result.current.start());
    await waitFor(() => expect(onFailed).toHaveBeenCalledWith("denied"));
    expect(onFinished).not.toHaveBeenCalled();
    expect(result.current.seconds).toBeNull();
  });

  it("tells another failure apart from a denied microphone", async () => {
    (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mockRejectedValue(
      new DOMException("busy", "NotReadableError"),
    );
    const onFailed = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed }));
    act(() => result.current.start());
    await waitFor(() => expect(onFailed).toHaveBeenCalledWith("other"));
  });

  it("says unsupported when this browser cannot record at all", async () => {
    vi.stubGlobal("MediaRecorder", undefined);
    const onFailed = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed }));
    act(() => result.current.start());
    expect(onFailed).toHaveBeenCalledWith("unsupported");
  });

  it("stops at once if the button was released before the microphone opened", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    act(() => {
      result.current.start();
      result.current.stop();
    });
    await waitFor(() => expect(onFinished).toHaveBeenCalled());
  });
});
