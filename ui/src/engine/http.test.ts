import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineError } from "./client";
import { HttpEngine } from "./http";
import { rememberFile } from "./previewFiles";

class FakeSource {
  static all: FakeSource[] = [];
  readyState = 1;
  closed = false;
  private readonly handlers = new Map<string, Array<(event: unknown) => void>>();
  constructor(readonly url: string) {
    FakeSource.all.push(this);
  }
  addEventListener(name: string, cb: (event: unknown) => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), cb]);
  }
  close() {
    this.closed = true;
    this.readyState = 2;
  }
  emit(name: string, data?: unknown) {
    for (const cb of this.handlers.get(name) ?? []) {
      cb({ data: data === undefined ? undefined : JSON.stringify(data) });
    }
  }
}

type Call = { url: string; init: RequestInit };

function fakeFetch(answer: (call: Call) => { status?: number; body: unknown }) {
  const calls: Call[] = [];
  const fn = vi.fn(async (url: string, init: RequestInit) => {
    const call = { url, init };
    calls.push(call);
    const { status = 200, body } = answer(call);
    return {
      ok: status >= 200 && status < 300,
      status,
      statusText: "status text",
      json: async () => body,
    } as Response;
  });
  return { fn: fn as unknown as typeof fetch, calls };
}

const ticketOr = (other: (call: Call) => { status?: number; body: unknown }) => (call: Call) =>
  call.url === "/api/ticket" ? { body: { ticket: `tk${FakeSource.all.length}` } } : other(call);

function engine(
  answer: (call: Call) => { status?: number; body: unknown },
  extra: Partial<ConstructorParameters<typeof HttpEngine>[0]> = {},
) {
  const f = fakeFetch(ticketOr(answer));
  const e = new HttpEngine({
    fetch: f.fn,
    EventSource: FakeSource as unknown as typeof EventSource,
    token: () => "secret",
    retryMs: () => 100,
    ...extra,
  });
  return { e, calls: f.calls };
}

const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
  vi.useFakeTimers();
  FakeSource.all = [];
});
afterEach(() => {
  vi.useRealTimers();
});

describe("HttpEngine commands", () => {
  it("posts the command's arguments with the bearer token and returns the answer", async () => {
    const { e, calls } = engine(() => ({ body: { id: "a" } }));
    await flush();
    const kept = await e.keepText("a note", "quickCapture");
    expect(kept).toEqual({ id: "a" });
    const call = calls.find((c) => c.url === "/api/invoke/keep_text");
    expect(call?.init.method).toBe("POST");
    expect(call?.init.body).toBe(JSON.stringify({ text: "a note", source: "quickCapture" }));
    expect(call?.init.headers).toMatchObject({ authorization: "Bearer secret" });
    e.dispose();
  });

  it("uses the snake_case command and camelCase keys", async () => {
    const { e, calls } = engine(() => ({ body: null }));
    await flush();
    await e.setItemKeepForever("i1", true);
    const call = calls.find((c) => c.url === "/api/invoke/set_item_keep_forever");
    expect(call?.init.body).toBe(JSON.stringify({ id: "i1", keepForever: true }));
    e.dispose();
  });

  it("turns a failure with a code into an EngineError", async () => {
    const { e } = engine(() => ({ status: 404, body: { code: "notFound", detail: "that item" } }));
    await flush();
    const error = await e.getItem("x").catch((err) => err);
    expect(error).toBeInstanceOf(EngineError);
    expect(error).toMatchObject({ code: "notFound", detail: "that item" });
    e.dispose();
  });

  it("says unauthorized for a refusal with no body, and unavailable when the server is gone", async () => {
    const { e } = engine(() => ({ status: 401, body: null }));
    await flush();
    await expect(e.getStatus()).rejects.toMatchObject({ code: "unauthorized" });
    e.dispose();
    const down = new HttpEngine({
      fetch: (async () => {
        throw new TypeError("network down");
      }) as unknown as typeof fetch,
      EventSource: FakeSource as unknown as typeof EventSource,
      retryMs: () => 1000,
    });
    await expect(down.getStatus()).rejects.toMatchObject({ code: "unavailable" });
    down.dispose();
  });

  it("sends a recording as the raw body with its mime type", async () => {
    const { e, calls } = engine(() => ({ body: { item: { id: "r" } } }));
    await flush();
    const bytes = new Uint8Array([1, 2, 3]);
    await e.keepRecording(bytes, "audio/webm");
    const call = calls.find((c) => c.url === "/api/invoke/keep_recording");
    expect(call?.init.body).toBe(bytes);
    expect(call?.init.headers).toMatchObject({
      "x-nocturne-mime": "audio/webm",
      "content-type": "application/octet-stream",
    });
    await e.keepBytes(bytes, "image/png", "paste");
    const other = calls.find((c) => c.url === "/api/invoke/keep_bytes");
    expect(other?.init.headers).toMatchObject({
      "x-nocturne-mime": "image/png",
      "x-nocturne-source": "paste",
    });
    e.dispose();
  });

  it("keeps a chosen file as bytes, once, and refuses a key it never gave out", async () => {
    const { e, calls } = engine(() => ({ body: { item: { id: "f" } } }));
    await flush();
    const key = rememberFile(new File(["hello"], "note.txt", { type: "text/plain" }));
    await e.keepFiles([key], "drop");
    const call = calls.find((c) => c.url === "/api/invoke/keep_bytes");
    expect(call?.init.headers).toMatchObject({
      "x-nocturne-mime": "text/plain",
      "x-nocturne-source": "drop",
    });
    await expect(e.keepFiles([key], "drop")).rejects.toMatchObject({ code: "invalid" });
    await expect(e.keepFiles(["/etc/passwd"], "drop")).rejects.toMatchObject({ code: "invalid" });
    e.dispose();
  });
});

describe("HttpEngine events", () => {
  it("opens the stream with a ticket and fans events out to listeners", async () => {
    const { e, calls } = engine(() => ({ body: null }));
    await flush();
    expect(calls[0]?.url).toBe("/api/ticket");
    expect(calls[0]?.init.headers).toMatchObject({ authorization: "Bearer secret" });
    const source = FakeSource.all[0] as FakeSource;
    expect(source.url).toBe("/api/events?ticket=tk0");
    const heard = vi.fn();
    const off = e.on("item-changed", heard);
    source.emit("item-changed", { id: "i1" });
    expect(heard).toHaveBeenCalledWith({ id: "i1" });
    off();
    source.emit("item-changed", { id: "i2" });
    expect(heard).toHaveBeenCalledTimes(1);
    const status = vi.fn();
    e.on("status-changed", status);
    source.emit("status-changed", { state: "ok" });
    expect(status).toHaveBeenCalledWith({ state: "ok" });
    e.dispose();
  });

  it("opens a new stream with a new ticket after an error", async () => {
    const { e, calls } = engine(() => ({ body: null }));
    await flush();
    (FakeSource.all[0] as FakeSource).emit("error");
    expect((FakeSource.all[0] as FakeSource).closed).toBe(true);
    await vi.advanceTimersByTimeAsync(150);
    expect(FakeSource.all).toHaveLength(2);
    expect(FakeSource.all[1]?.url).toBe("/api/events?ticket=tk1");
    expect(calls.filter((c) => c.url === "/api/ticket")).toHaveLength(2);
    e.dispose();
  });

  it("keeps trying when the ticket cannot be had, and stops when disposed", async () => {
    let up = false;
    const f = fakeFetch((call) =>
      call.url === "/api/ticket"
        ? up
          ? { body: { ticket: "late" } }
          : { status: 503, body: { code: "unavailable", detail: null } }
        : { body: null },
    );
    const e = new HttpEngine({
      fetch: f.fn,
      EventSource: FakeSource as unknown as typeof EventSource,
      retryMs: () => 100,
    });
    await vi.advanceTimersByTimeAsync(350);
    expect(FakeSource.all).toHaveLength(0);
    up = true;
    await vi.advanceTimersByTimeAsync(150);
    expect(FakeSource.all).toHaveLength(1);
    e.dispose();
    (FakeSource.all[0] as FakeSource).emit("error");
    await vi.advanceTimersByTimeAsync(500);
    expect(FakeSource.all).toHaveLength(1);
  });

  it("ignores a payload that is not JSON", async () => {
    const { e } = engine(() => ({ body: null }));
    await flush();
    const heard = vi.fn();
    e.on("item-changed", heard);
    const source = FakeSource.all[0] as FakeSource;
    for (const cb of (
      source as unknown as { handlers: Map<string, Array<(e: unknown) => void>> }
    ).handlers.get("item-changed") ?? []) {
      cb({ data: "{not json" });
    }
    expect(heard).not.toHaveBeenCalled();
    e.dispose();
  });
});
