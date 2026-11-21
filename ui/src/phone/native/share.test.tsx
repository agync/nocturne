import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockPhoneEngine } from "../mock";
import { memoryQueue } from "../queue";
import { sharedFile, takeLaunch, takeShared } from "./launch";
import { ShareSheet } from "./screens/ShareSheet";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

describe("what the app was opened for", () => {
  it("takes only the shortcuts it knows", async () => {
    mockIPC((cmd) => (cmd === "plugin:nocturne-system|take_launch" ? "record" : null));
    expect(await takeLaunch()).toBe("record");
    mockIPC((cmd) => (cmd === "plugin:nocturne-system|take_launch" ? "settings" : null));
    expect(await takeLaunch()).toBeNull();
  });

  it("drops a share with nothing in it, and reads a file's bytes", async () => {
    mockIPC((cmd) => {
      if (cmd === "plugin:nocturne-system|take_shared")
        return { text: "  ", subject: null, files: [] };
      if (cmd === "plugin:nocturne-system|shared_file") return [1, 2, 3];
    });
    expect(await takeShared()).toBeNull();
    const blob = await sharedFile({ name: "0-a.png", shown: "a.png", mime: "image/png", size: 3 });
    expect(blob.size).toBe(3);
    expect(blob.type).toBe("image/png");
  });
});

describe("keeping what another app shared", () => {
  const shared = { text: "https://example.org/letter", subject: "Trip letter", files: [] };

  it("keeps the text with the note before it", async () => {
    mockIPC(() => null);
    const engine = createMockPhoneEngine();
    await engine.connect();
    const keep = vi.spyOn(engine, "keepText");
    const queue = memoryQueue();
    render(
      <ShareSheet
        shared={shared}
        engine={engine}
        queue={queue}
        connection={engine.connection()}
        now={() => 0}
        onQueued={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText("Add a note (optional)"), {
      target: { value: "Due Monday" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByRole("heading", { name: "Kept." })).toBeInTheDocument();
    expect(keep.mock.calls[0]?.[0]).toBe("Due Monday\n\nTrip letter\n\nhttps://example.org/letter");
    expect(keep.mock.calls[0]?.[2]).toMatchObject({ web: true });
    expect(await queue.count()).toBe(0);
  });

  const file = { name: "0-a.bin", shown: "a.bin", mime: "application/zip", size: 4 };

  it("says Nocturne can't keep a file it does not keep, and queues nothing", async () => {
    mockIPC((cmd) => (cmd === "plugin:nocturne-system|shared_file" ? [80, 75, 3, 4] : null));
    const engine = createMockPhoneEngine();
    await engine.connect();
    const queue = memoryQueue();
    render(
      <ShareSheet
        shared={{ text: null, subject: null, files: [file] }}
        engine={engine}
        queue={queue}
        connection={engine.connection()}
        now={() => 0}
        onQueued={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(
      await screen.findByRole("heading", { name: "Nocturne can’t keep that." }),
    ).toBeInTheDocument();
    expect(await queue.count()).toBe(0);
  });

  it("says so when a shared file can no longer be read", async () => {
    mockIPC((cmd) => {
      if (cmd === "plugin:nocturne-system|shared_file") throw "gone";
    });
    const engine = createMockPhoneEngine();
    await engine.connect();
    render(
      <ShareSheet
        shared={{ text: null, subject: null, files: [file] }}
        engine={engine}
        queue={memoryQueue()}
        connection={engine.connection()}
        now={() => 0}
        onQueued={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(
      await screen.findByRole("heading", { name: "That didn’t go across." }),
    ).toBeInTheDocument();
  });

  it("waits on the phone while the computer is away", async () => {
    mockIPC(() => null);
    const engine = createMockPhoneEngine({ connection: "asleep" });
    const queue = memoryQueue();
    render(
      <ShareSheet
        shared={shared}
        engine={engine}
        queue={queue}
        connection={{ kind: "asleep" }}
        now={() => 0}
        onQueued={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(await screen.findByRole("heading", { name: "Waiting." })).toBeInTheDocument();
    expect(await queue.count()).toBe(1);
  });
});
