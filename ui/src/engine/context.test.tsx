import type { QueryClient } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { AskDelta } from "./bindings";
import {
  createQueryClient,
  EngineProvider,
  EVENT_INVALIDATIONS,
  useAsk,
  usePage,
  useStatus,
} from "./context";
import { createMockEngine } from "./mock/engine";

const NOW = new Date("2026-11-13T07:02:00Z");

function wrapper(engine: ReturnType<typeof createMockEngine>, client?: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <EngineProvider engine={engine} queryClient={client}>
      {children}
    </EngineProvider>
  );
}

describe("event to query invalidation", () => {
  it("maps each event to the queries it makes stale (B3)", () => {
    expect(EVENT_INVALIDATIONS).toEqual({
      "status-changed": [["status"]],
      "item-changed": [["items"], ["page"], ["needsYou"]],
      "page-ready": [["page"], ["pages"]],
      "needs-you-changed": [["needsYou"], ["status"]],
      "setup-progress": [["setup"]],
      "export-progress": [],
    });
  });

  it("refreshes the status when the engine says something needs the person", async () => {
    const engine = createMockEngine({ now: NOW });
    const { result } = renderHook(() => useStatus(), { wrapper: wrapper(engine) });
    await waitFor(() => expect(result.current.data?.needsYou).toBe(2));
    await act(async () => {
      await engine.markDone("mem-trip");
    });
    await waitFor(() => expect(result.current.data?.needsYou).toBe(1));
  });

  it("refreshes the page when an item changes", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const engine = createMockEngine({ now: NOW });
    const client = createQueryClient();
    const spy = vi.spyOn(client, "invalidateQueries");
    renderHook(() => usePage(), { wrapper: wrapper(engine, client) });
    await engine.keepText("A thought", "quickCapture");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(spy).toHaveBeenCalledWith({ queryKey: ["page"] });
    vi.useRealTimers();
  });
});

describe("useAsk", () => {
  it("follows an answer from the first step to its end", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const engine = createMockEngine({ now: NOW });
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    expect(result.current.phase).toBe("idle");
    await act(async () => {
      await result.current.ask("When is the dentist?");
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(result.current.phase).toBe("done");
    expect(result.current.text).toContain("Dr Chen");
    expect(result.current.receipts.length).toBeGreaterThan(0);
    vi.useRealTimers();
  });

  it("keeps deltas that arrive before ask() has returned", async () => {
    const engine = createMockEngine({ now: NOW });
    const early: AskDelta = {
      askId: "fast-1",
      phase: "searching",
      textDelta: null,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
    };
    const emit = (engine as unknown as { emit: (n: string, p: unknown) => void }).emit.bind(engine);
    engine.ask = async () => {
      emit("ask-delta", early);
      emit("ask-delta", { ...early, phase: "done", textDelta: "Yes." });
      await Promise.resolve();
      return "fast-1";
    };
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    await act(async () => {
      await result.current.ask("Quick one");
    });
    expect(result.current.deltas.map((d) => d.phase)).toEqual(["searching", "done"]);
    expect(result.current.text).toBe("Yes.");
    expect(result.current.phase).toBe("done");
  });

  it("ends in nothing when nothing matches", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const engine = createMockEngine({ now: NOW });
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    await act(async () => {
      await result.current.ask("passport");
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.phase).toBe("nothing");
    vi.useRealTimers();
  });

  it("resets and exposes the error when the engine refuses the question", async () => {
    const engine = createMockEngine({ now: NOW });
    engine.ask = async () => {
      throw new Error("not ready");
    };
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    let id: string | null = "x";
    await act(async () => {
      id = await result.current.ask("Anything?");
    });
    expect(id).toBeNull();
    expect(result.current.error?.message).toBe("not ready");
    expect(result.current.phase).toBe("idle");
  });

  it("keeps two overlapping questions apart, even when the first answers last", async () => {
    const engine = createMockEngine({ now: NOW });
    const emit = (engine as unknown as { emit: (n: string, p: unknown) => void }).emit.bind(engine);
    const delta = (
      askId: string,
      phase: AskDelta["phase"],
      textDelta: string | null,
    ): AskDelta => ({
      askId,
      phase,
      textDelta,
      receipts: [],
      memoryIds: [],
      near: [],
      found: null,
      etaSeconds: null,
    });
    const resolvers: Record<string, (id: string) => void> = {};
    engine.ask = (question: string) =>
      new Promise<string>((resolve) => {
        resolvers[question] = resolve;
      });
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    let first: Promise<string | null> = Promise.resolve(null);
    let second: Promise<string | null> = Promise.resolve(null);
    act(() => {
      first = result.current.ask("first");
    });
    act(() => {
      second = result.current.ask("second");
    });
    act(() => {
      emit("ask-delta", delta("A", "streaming", "from A"));
      emit("ask-delta", delta("B", "streaming", "from B"));
    });
    await act(async () => {
      resolvers.second?.("B");
      await second;
    });
    await act(async () => {
      resolvers.first?.("A");
      await first;
    });
    act(() => {
      emit("ask-delta", delta("A", "done", " late A"));
      emit("ask-delta", delta("B", "done", " and more B"));
    });
    expect(result.current.askId).toBe("B");
    expect(result.current.text).toBe("from B and more B");
  });

  it("never buffers more than a bounded number of early deltas", async () => {
    const engine = createMockEngine({ now: NOW });
    const emit = (engine as unknown as { emit: (n: string, p: unknown) => void }).emit.bind(engine);
    let release: (id: string) => void = () => undefined;
    engine.ask = () => new Promise<string>((r) => (release = r));
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    let pending: Promise<string | null> = Promise.resolve(null);
    act(() => {
      pending = result.current.ask("flood");
    });
    act(() => {
      for (let i = 0; i < 1000; i += 1) {
        emit("ask-delta", {
          askId: "F",
          phase: "streaming",
          textDelta: "x",
          receipts: [],
          memoryIds: [],
          near: [],
          found: null,
          etaSeconds: null,
        });
      }
    });
    await act(async () => {
      release("F");
      await pending;
    });
    expect(result.current.deltas.length).toBeLessThanOrEqual(200);
  });
});

describe("shell hand-offs", () => {
  it("tells the shell the first screen has painted, once", async () => {
    const engine = createMockEngine({ now: NOW });
    const ready = vi.spyOn(engine, "appReady");
    renderHook(() => useStatus(), { wrapper: wrapper(engine) });
    await waitFor(() => expect(ready).toHaveBeenCalledTimes(1));
  });

  it("stops the answer on screen before the next question starts", async () => {
    const engine = createMockEngine({ now: NOW });
    const cancel = vi.spyOn(engine, "cancelAsk");
    const { result } = renderHook(() => useAsk(), { wrapper: wrapper(engine) });
    let first: string | null = null;
    await act(async () => {
      first = await result.current.ask("passport");
    });
    expect(cancel).not.toHaveBeenCalled();
    await act(async () => {
      await result.current.ask("dentist");
    });
    expect(cancel).toHaveBeenCalledWith(first);
  });
});
