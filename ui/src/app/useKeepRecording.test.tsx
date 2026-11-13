import { act, renderHook, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/Toast/Toast";
import { EngineProvider } from "../engine/context";
import { createMockEngine } from "../engine/mock/engine";
import { useKeepRecording } from "./useKeepRecording";

class FakeRecorder {
  mimeType = "audio/webm";
  state = "inactive";
  ondataavailable: ((e: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  start() {
    this.state = "recording";
  }
  stop() {
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob([new Uint8Array([9, 8, 7])]) });
    this.onstop?.();
  }
}

beforeEach(() => {
  vi.stubGlobal("MediaRecorder", FakeRecorder);
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }) },
  });
});
afterEach(() => vi.unstubAllGlobals());

const wrap =
  (engine: ReturnType<typeof createMockEngine>) =>
  ({ children }: { children: ReactNode }) => (
    <EngineProvider engine={engine}>
      <ToastProvider>{children}</ToastProvider>
    </EngineProvider>
  );

const record = async (result: { current: ReturnType<typeof useKeepRecording> }) => {
  act(() => result.current.start());
  await waitFor(() => expect(result.current.seconds).toBe(0));
  act(() => result.current.stop());
};

describe("useKeepRecording", () => {
  it("keeps the note and says so", async () => {
    const engine = createMockEngine();
    const spy = vi.spyOn(engine, "keepRecording");
    const { result } = renderHook(() => useKeepRecording(), { wrapper: wrap(engine) });
    await record(result);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(document.body).toHaveTextContent(/Voice note, 0:0\d\. Reading it now\./);
  });

  it("does not lose the recording when keeping fails: Try again sends the same audio", async () => {
    const engine = createMockEngine();
    const spy = vi
      .spyOn(engine, "keepRecording")
      .mockRejectedValueOnce(new Error("busy"))
      .mockImplementation(async (bytes, mime) => {
        void bytes;
        void mime;
        return { type: "new", item: await engine.getItem("item-sam") };
      });
    const { result } = renderHook(() => useKeepRecording(), { wrapper: wrap(engine) });
    await record(result);
    const retry = await screen_findButton("Try again");
    expect(document.body).toHaveTextContent(
      "I couldn’t keep that voice note just now. It is still here.",
    );
    await userEvent.click(retry);
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    expect(Array.from(spy.mock.calls[1]?.[0] ?? [])).toEqual([9, 8, 7]);
    expect(Array.from(spy.mock.calls[0]?.[0] ?? [])).toEqual([9, 8, 7]);
  });

  it("explains a denied microphone and an unsupported one in different words", async () => {
    const { result } = renderHook(() => useKeepRecording(), { wrapper: wrap(createMockEngine()) });
    (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mockRejectedValue(
      new DOMException("no", "NotAllowedError"),
    );
    act(() => result.current.start());
    await waitFor(() =>
      expect(document.body).toHaveTextContent(
        "Nocturne can’t hear you. Microphone access is turned off for it.",
      ),
    );
    vi.stubGlobal("MediaRecorder", undefined);
    act(() => result.current.start());
    await waitFor(() =>
      expect(document.body).toHaveTextContent("This computer can’t record sound inside Nocturne."),
    );
  });
});

async function screen_findButton(name: string): Promise<HTMLElement> {
  return waitFor(() => {
    const b = [...document.querySelectorAll("button")].find((x) => x.textContent === name);
    if (!b) throw new Error(`no ${name}`);
    return b as HTMLElement;
  });
}
