import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installFakeAudio, speak } from "../test/fakeAudio";
import { recordingState, useRecorder } from "./useRecorder";

const track = { stop: vi.fn() };

beforeEach(() => {
  installFakeAudio();
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
  it("records while held and hands over a 16 kHz WAV when released", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    expect(result.current.seconds).toBeNull();
    act(() => result.current.start());
    await waitFor(() => expect(result.current.seconds).toBe(0));
    speak(1000);
    act(() => result.current.stop());
    expect(onFinished).toHaveBeenCalledOnce();
    const [bytes, mime] = onFinished.mock.calls[0] as [Uint8Array, string, number];
    expect(mime).toBe("audio/wav");
    expect(String.fromCharCode(...bytes.subarray(0, 4))).toBe("RIFF");
    expect(String.fromCharCode(...bytes.subarray(8, 12))).toBe("WAVE");
    const view = new DataView(bytes.buffer, bytes.byteOffset);
    expect(view.getUint32(24, true)).toBe(16000);
    expect(bytes.length).toBeGreaterThan(30_000);
    expect(result.current.seconds).toBeNull();
    expect(track.stop).toHaveBeenCalled();
  });

  it("does not depend on MediaRecorder at all", async () => {
    vi.stubGlobal("MediaRecorder", undefined);
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    expect(result.current.unavailable).toBeNull();
    act(() => result.current.start());
    await waitFor(() => expect(result.current.seconds).toBe(0));
    speak(300);
    act(() => result.current.stop());
    expect(onFinished).toHaveBeenCalledOnce();
  });

  it("keeps nothing when no sound arrived", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    act(() => result.current.start());
    await waitFor(() => expect(result.current.seconds).toBe(0));
    act(() => result.current.stop());
    expect(onFinished).not.toHaveBeenCalled();
    expect(result.current.seconds).toBeNull();
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

  it("says unsupported when this window has no way to listen", async () => {
    vi.stubGlobal("AudioContext", undefined);
    const onFailed = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed }));
    act(() => result.current.start());
    expect(onFailed).toHaveBeenCalledWith("unsupported");
  });

  it("says so once when recording is impossible, and then stays quiet", () => {
    vi.stubGlobal("AudioContext", undefined);
    const onFailed = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed }));
    expect(result.current.unavailable).toBe("unsupported");
    act(() => result.current.start());
    act(() => result.current.start());
    act(() => result.current.start());
    expect(onFailed).toHaveBeenCalledOnce();
    expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
  });

  it("tells a page that is not secure from a window that cannot record", () => {
    vi.stubGlobal("isSecureContext", false);
    expect(recordingState()).toBe("not-secure");
    const onFailed = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed }));
    expect(result.current.unavailable).toBe("insecure");
    act(() => result.current.start());
    expect(onFailed).toHaveBeenCalledWith("insecure");
  });

  it("is available when the window can record", () => {
    expect(recordingState()).toBe("available");
    const { result } = renderHook(() => useRecorder({ onFinished: vi.fn(), onFailed: vi.fn() }));
    expect(result.current.unavailable).toBeNull();
  });

  it("tells missing devices from missing audio", () => {
    vi.stubGlobal("AudioContext", undefined);
    expect(recordingState()).toBe("no-recorder");
    vi.unstubAllGlobals();
    installFakeAudio();
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: undefined });
    expect(recordingState()).toBe("no-devices");
  });

  it("stops at once if the button was released before the microphone opened", async () => {
    const onFinished = vi.fn();
    const { result } = renderHook(() => useRecorder({ onFinished, onFailed: vi.fn() }));
    act(() => {
      result.current.start();
      result.current.stop();
    });
    await waitFor(() => expect(result.current.seconds).toBeNull());
    expect(track.stop).toHaveBeenCalled();
    expect(onFinished).not.toHaveBeenCalled();
  });
});
