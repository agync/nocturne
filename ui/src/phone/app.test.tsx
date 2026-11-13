import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PhoneKept } from "../engine/bindings";
import { FakeAudioContext, installFakeAudio, speak } from "../test/fakeAudio";
import { App } from "./App";
import { type PhoneEngine, PhoneRequestError } from "./engine";
import { createMockPhoneEngine, type MockScript } from "./mock";
import { memoryQueue } from "./queue";
import { Ask, capQuestion, useAskSession } from "./screens/Ask";
import { browserRecorder, Keep } from "./screens/Keep";
import { Original } from "./screens/Original";
import { PageScreen } from "./screens/PageScreen";
import type { Services } from "./services";
import { mockServices } from "./testing/mockServices";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function open(query: string, tweak?: (s: Services) => void): Promise<Services> {
  window.history.replaceState(null, "", `/?${query}`);
  const services = await mockServices();
  // jsdom has no storage or secure context, which the real check would call too old.
  services.unsupported ??= [];
  tweak?.(services);
  render(<App services={services} />);
  return services;
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });

describe("home", () => {
  it("home.connected_three_actions", async () => {
    await open("s=connected");
    expect(await screen.findByText("Connected to Home computer")).toBeInTheDocument();
    for (const name of [/^Ask/, /^Keep/, /^Today’s page/]) {
      expect(button(name)).not.toHaveAttribute("aria-disabled");
    }
    expect(await screen.findByText("Two things need you today.")).toBeInTheDocument();
    expect(
      screen.getByText("Things you marked private stay on your computer."),
    ).toBeInTheDocument();
  });

  it("home.asleep_disables_ask_and_page", async () => {
    await open("s=asleep");
    expect(
      await screen.findByText("Home computer is asleep, switched off or has a new address."),
    ).toBeInTheDocument();
    expect(button(/^Ask/)).toHaveAttribute("aria-disabled", "true");
    expect(button(/^Today’s page/)).toHaveAttribute("aria-disabled", "true");
    expect(button(/^Keep/)).not.toHaveAttribute("aria-disabled");
    expect(await within(button(/^Keep/)).findByText("2 waiting")).toBeInTheDocument();
    expect(screen.getByText("Last connected 07:48 · tries again by itself")).toBeInTheDocument();
    await userEvent.click(button(/^Ask/));
    expect(screen.queryByRole("heading", { name: "Ask" })).not.toBeInTheDocument();
  });

  it("Most private adds the Tailscale sentence", async () => {
    await open("s=asleepMostPrivate");
    expect(
      await screen.findByText("If you use Most private, check Tailscale is on."),
    ).toBeInTheDocument();
  });

  it("home.cant_connect_keeps_pairing", async () => {
    const services = await open("s=cantConnect");
    expect(
      await screen.findByText("Can’t connect to Home computer right now."),
    ).toBeInTheDocument();
    expect(await services.store.device()).not.toBeNull();
  });
});

describe("offline and removed", () => {
  it("offline.asleep_or_new_address: one state, with Scan the code for Simple and the Tailscale sentence for Most private", async () => {
    await open("s=asleep");
    expect(
      await screen.findByText("Home computer is asleep, switched off or has a new address."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Has your computer got a new address?" }),
    ).toBeInTheDocument();
    expect(button("Scan the new code")).toBeInTheDocument();
    expect(button("Type the words instead")).toBeInTheDocument();
    expect(
      screen.queryByText("If you use Most private, check Tailscale is on."),
    ).not.toBeInTheDocument();
    cleanup();
    await open("s=asleepMostPrivate");
    expect(
      await screen.findByText("If you use Most private, check Tailscale is on."),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Has your computer got a new address?" }),
    ).not.toBeInTheDocument();
  });

  it("offline.outdated: an old phone app asks to add the phone again and keeps the key", async () => {
    const services = await open("s=outdated");
    expect(
      await screen.findByRole("heading", { name: "This phone needs adding again." }),
    ).toBeInTheDocument();
    expect(await services.store.device()).not.toBeNull();
    await userEvent.click(button("Type the code"));
    expect(
      await screen.findByRole("heading", { name: "Type the code from your computer" }),
    ).toBeInTheDocument();
    expect(await services.store.device()).not.toBeNull();
  });

  it("offline.turned_off", async () => {
    await open("s=turnedOff");
    expect(
      await screen.findByRole("heading", { name: "Away from home is off." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/This phone is still paired/)).toBeInTheDocument();
    expect(screen.getByText("Last connected 07:48")).toBeInTheDocument();
  });

  it("removed.offers_to_clear_waiting", async () => {
    const services = await open("s=removed&waiting=2");
    expect(
      await screen.findByRole("heading", { name: "This phone was removed from Nocturne." }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("2 things were still waiting on this phone."),
    ).toBeInTheDocument();
    await userEvent.click(button("Clear them from this phone…"));
    expect(screen.getByRole("heading", { name: "Clear what’s waiting?" })).toBeInTheDocument();
    expect(await services.queue.count()).toBe(2);
    await userEvent.click(button("Clear them"));
    await waitFor(async () => expect(await services.queue.count()).toBe(0));
  });
});

describe("the settings sheet", () => {
  it("sheet.disconnect_confirms_and_forgets_keys", async () => {
    const services = await open("s=connected");
    await userEvent.click(await screen.findByRole("button", { name: "Menu" }));
    const dialog = screen.getByRole("dialog", { name: "This phone" });
    expect(within(dialog).getByText("Pixel")).toBeInTheDocument();
    expect(within(dialog).getByText("Simple, through Cloudflare")).toBeInTheDocument();
    expect(within(dialog).getByText("Nocturne 0.1.0")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Disconnect this phone…" }));
    expect(screen.getByRole("heading", { name: "Disconnect this phone?" })).toBeInTheDocument();
    expect(await services.store.device()).not.toBeNull();
    await userEvent.click(button("Stay connected"));
    expect(await services.store.device()).not.toBeNull();
    await userEvent.click(button("Disconnect this phone…"));
    await userEvent.click(button("Disconnect"));
    expect(
      await screen.findByRole("heading", { name: "Add this phone from your computer" }),
    ).toBeInTheDocument();
    expect(await services.store.device()).toBeNull();
    expect(
      screen.queryByText("Your computer may still list this phone. Remove it there too."),
    ).not.toBeInTheDocument();
  });

  it("sheet.disconnect_unreachable_says_the_computer_may_still_list_it", async () => {
    const services = await open("s=asleep");
    await userEvent.click(await screen.findByRole("button", { name: "Menu" }));
    await userEvent.click(button("Disconnect this phone…"));
    await userEvent.click(button("Disconnect"));
    expect(
      await screen.findByRole("heading", { name: "Add this phone from your computer" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Your computer may still list this phone. Remove it there too."),
    ).toBeInTheDocument();
    expect(await services.store.device()).toBeNull();
  });
});

describe("pairing screens", () => {
  it("pair.name_then_words_then_allowed", async () => {
    await open("s=pair");
    expect(await screen.findByRole("heading", { name: "Name this phone" })).toHaveFocus();
    const name = screen.getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Pixel");
    await userEvent.click(button("Connect"));
    expect(
      await screen.findByRole("heading", {
        name: "Check your computer shows the same three words.",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("list", { name: "Check words: harbour, maple, lantern" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/this phone isn’t showing three words, someone else is trying to pair/),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: "Pixel can now use Nocturne." }),
    ).toBeInTheDocument();
    await userEvent.click(button("Not now, open Nocturne"));
    expect(await screen.findByText("Connected to Home computer")).toBeInTheDocument();
  });

  it("pair.refused_shows_start_again, pair.expired_code and pair.spent_code", async () => {
    for (const [scenario, title] of [
      ["pairRefused", "Your computer said no."],
      ["pairExpired", "This code has run out."],
      ["pairSpent", "This code can’t be used again."],
    ] as const) {
      await open(`s=${scenario}`);
      await userEvent.click(await screen.findByRole("button", { name: "Connect" }));
      expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();
      expect(button("Start again")).toBeInTheDocument();
      cleanup();
    }
  });

  it("pair.device_name_capped: the field stops at forty bytes", async () => {
    await open("s=pair");
    const name = await screen.findByLabelText("Name");
    fireEvent.change(name, { target: { value: "é".repeat(30) } });
    expect(new TextEncoder().encode((name as HTMLInputElement).value).length).toBeLessThanOrEqual(
      40,
    );
    fireEvent.change(name, { target: { value: "Pixel‮ 9" } });
    expect((name as HTMLInputElement).value).toBe("Pixel 9");
  });

  it("pair.ios_safari_asks_home_screen_first", async () => {
    await open("s=iphone");
    expect(
      await screen.findByRole("heading", { name: "Add Nocturne to your Home Screen" }),
    ).toBeInTheDocument();
    expect(screen.getByText("You may need to pair once more")).toBeInTheDocument();
    await userEvent.click(button("Not now, open Nocturne"));
    expect(await screen.findByRole("heading", { name: "Name this phone" })).toBeInTheDocument();
    cleanup();
    await open("s=iphone");
    await userEvent.click(await screen.findByRole("button", { name: "Done" }));
    expect(
      await screen.findByRole("heading", { name: "Add this phone from your computer" }),
    ).toBeInTheDocument();
  });

  it("the typed code needs all twenty-four characters", async () => {
    await open("s=typed");
    expect(
      await screen.findByRole("heading", { name: "Type the code from your computer" }),
    ).toBeInTheDocument();
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
    const groups = [1, 2, 3, 4].map((n) => screen.getByLabelText(`Group ${n}`) as HTMLInputElement);
    for (const [i, g] of groups.entries())
      fireEvent.change(g, { target: { value: i < 3 ? "abc123" : "abc" } });
    expect(groups[0]?.value).toBe("ABC123");
    expect(button("Continue")).toHaveAttribute("aria-disabled", "true");
    fireEvent.change(groups[3] as HTMLInputElement, { target: { value: "xyz789" } });
    expect(button("Continue")).not.toHaveAttribute("aria-disabled");
  });

  it("relink.system_camera_page_points_home", async () => {
    await open("s=relinkHere");
    expect(
      await screen.findByRole("heading", { name: "Open Nocturne from your home screen" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/choose “Scan the new code”/)).toBeInTheDocument();
  });

  it("support.old_browser_state", async () => {
    await open("s=tooOld");
    expect(
      await screen.findByRole("heading", { name: "This browser is too old" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

const engineWith = (script: MockScript = {}, overrides: Partial<PhoneEngine> = {}) => ({
  ...createMockPhoneEngine(script),
  ...overrides,
});

const kept: PhoneKept = {
  item: {
    id: "item-1",
    kind: "text",
    title: "Bin day",
    keptAt: "2026-11-13T14:05:00.000Z",
    durationMs: null,
    pages: null,
    hasOriginal: false,
    summary: null,
    text: null,
    truncated: false,
  },
  duplicate: false,
  takeBackUntil: "2026-11-13T14:15:00.000Z",
};

describe("keep", () => {
  it("keep.text_kept_shows_take_back", async () => {
    await open("s=connected");
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    expect(button("Keep")).toHaveAttribute("aria-disabled", "true");
    await userEvent.type(screen.getByLabelText("Type something to keep"), "Bin day moved");
    await userEvent.click(button("Keep"));
    expect(await screen.findByRole("heading", { name: "Kept." })).toBeInTheDocument();
    expect(screen.getByText(/kept from Pixel/)).toBeInTheDocument();
    expect(screen.getByText(/Changed your mind\? Until/)).toBeInTheDocument();
    await userEvent.click(button("Take back"));
    expect(await screen.findByText("Taken back. It’s no longer kept.")).toBeInTheDocument();
  });

  it("keep.photo_with_note_sends_caption and keep.caption_capped", async () => {
    const keepBytes = vi.fn(async () => kept);
    await open("s=connected", (s) => {
      s.engineFor = () => engineWith({}, { keepBytes });
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    const png = new File([Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 1, 2, 3)], "p.png", {
      type: "image/png",
    });
    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    await userEvent.upload(input, png);
    expect(await screen.findByRole("heading", { name: "Keep a photo" })).toBeInTheDocument();
    const note = screen.getByLabelText(/Add a note/) as HTMLInputElement;
    fireEvent.change(note, { target: { value: "x".repeat(150) } });
    expect([...note.value]).toHaveLength(120);
    fireEvent.change(note, { target: { value: "Leo’s trip form" } });
    await userEvent.click(button("Keep"));
    await waitFor(() => expect(keepBytes).toHaveBeenCalledOnce());
    const [blob, id, caption] = keepBytes.mock.calls[0] as unknown as [Blob, string, string];
    expect(blob.size).toBe(7);
    expect(id).toMatch(/^[A-Za-z0-9_-]{1,36}$/);
    expect(caption).toBe("Leo’s trip form");
    expect(await screen.findByRole("heading", { name: "Kept." })).toBeInTheDocument();
  });

  it("keep.written_to_the_queue_first: the entry is there while the keep is sent, and goes when it is kept", async () => {
    let during = -1;
    const services = await open("s=connected", (s) => {
      s.engineFor = () =>
        engineWith(
          {},
          {
            keepText: async () => {
              during = await s.queue.count();
              return kept;
            },
          },
        );
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    await userEvent.type(screen.getByLabelText("Type something to keep"), "Bin day moved");
    await userEvent.click(button("Keep"));
    expect(await screen.findByRole("heading", { name: "Kept." })).toBeInTheDocument();
    expect(during).toBe(1);
    expect(await services.queue.count()).toBe(0);
  });

  it("keep.busy_goes_back_to_the_queue: a busy or rate-limited refusal waits, and is not a failure", async () => {
    for (const code of ["busy", "tooMany", "notReady", "paused"] as const) {
      const services = await open("s=connected", (s) => {
        s.engineFor = () =>
          engineWith({}, { keepText: async () => Promise.reject(new PhoneRequestError(code)) });
      });
      await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
      await userEvent.type(screen.getByLabelText("Type something to keep"), "Bin day moved");
      await userEvent.click(button("Keep"));
      expect(await screen.findByRole("heading", { name: "Waiting." }), code).toBeInTheDocument();
      expect(await services.queue.count(), code).toBe(1);
      cleanup();
    }
  });

  it("keep.never_taken_leaves_the_queue: a refusal that will not change is a failure and does not wait", async () => {
    const services = await open("s=connected", (s) => {
      s.engineFor = () =>
        engineWith({}, { keepText: async () => Promise.reject(new PhoneRequestError("tooLarge")) });
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    await userEvent.type(screen.getByLabelText("Type something to keep"), "x");
    await userEvent.click(button("Keep"));
    expect(
      await screen.findByRole("heading", { name: "Too big to keep from a phone." }),
    ).toBeInTheDocument();
    expect(await services.queue.count()).toBe(0);
  });

  it("keep.too_large_before_send: nothing is sent", async () => {
    const keepBytes = vi.fn(async () => kept);
    await open("s=connected", (s) => {
      s.engineFor = () => engineWith({}, { keepBytes });
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    const big = new File([Uint8Array.of(0x89, 0x50, 0x4e, 0x47)], "big.png", { type: "image/png" });
    Object.defineProperty(big, "size", { value: 60_000_000 });
    await userEvent.upload(document.querySelector('input[type="file"]') as HTMLInputElement, big);
    await userEvent.click(button("Keep"));
    expect(
      await screen.findByRole("heading", { name: "Too big to keep from a phone." }),
    ).toBeInTheDocument();
    expect(screen.getByText(/limit is 50 MB/)).toBeInTheDocument();
    expect(keepBytes).not.toHaveBeenCalled();
  });

  it("a kind of file the computer will not take is said plainly", async () => {
    await open("s=connected");
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    const odd = new File(["<html>"], "x.png", { type: "image/png" });
    await userEvent.upload(document.querySelector('input[type="file"]') as HTMLInputElement, odd);
    await userEvent.click(button("Keep"));
    expect(
      await screen.findByRole("heading", { name: "Nocturne can’t keep that." }),
    ).toBeInTheDocument();
  });

  it("keep.asleep_queues_with_count", async () => {
    const services = await open("s=asleep");
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    await userEvent.type(
      screen.getByLabelText("Type something to keep"),
      "Bin day moved to Thursday",
    );
    await userEvent.click(button("Keep"));
    expect(await screen.findByRole("heading", { name: "Waiting." })).toBeInTheDocument();
    expect(await services.queue.count()).toBe(3);
    await userEvent.click(button("Home"));
    expect(
      within(await screen.findByRole("button", { name: /^Keep/ })).getByText("3 waiting"),
    ).toBeInTheDocument();
  });

  it("queue: what waited goes across, in order, when the phone connects", async () => {
    const keepText = vi.fn(async () => kept);
    const services = await open("s=connected&waiting=2", (s) => {
      s.engineFor = () => engineWith({}, { keepText });
    });
    await screen.findByText("Connected to Home computer");
    await waitFor(() => expect(keepText).toHaveBeenCalledTimes(2));
    expect(keepText.mock.calls.map((c) => (c as unknown as string[])[0])).toEqual([
      "Waiting note 1",
      "Waiting note 2",
    ]);
    expect(await screen.findByText("2 things you kept went across.")).toBeInTheDocument();
    expect(await services.queue.count()).toBe(0);
  });

  it("record.stop_and_keep: the recording is kept when stopped", async () => {
    const keepRecording = vi.fn(async () => ({
      ...kept,
      item: { ...kept.item, kind: "audio" as const },
    }));
    let stopped = 0;
    render(
      <Keep
        engine={engineWith({}, { keepRecording })}
        queue={memoryQueue()}
        connection={{
          kind: "connected",
          hello: {
            computerName: "Home computer",
            deviceName: "Pixel",
            connection: "simple",
            version: "0.1.0",
            minPhoneBuild: 1,
          },
        }}
        deviceName="Pixel"
        now={() => Date.now()}
        onHome={() => undefined}
        onQueued={() => undefined}
        recorder={async () => ({
          stop: async () => {
            stopped += 1;
            return new Blob([Uint8Array.of(0x1a, 0x45, 0xdf, 0xa3, 1, 2)], {
              type: "audio/webm;codecs=opus",
            });
          },
          cancel: () => undefined,
        })}
      />,
    );
    await userEvent.click(button("Record"));
    expect(await screen.findByText("Recording")).toBeInTheDocument();
    await waitFor(() => expect(button("Stop and keep")).not.toBeDisabled());
    await userEvent.click(button("Stop and keep"));
    await waitFor(() => expect(keepRecording).toHaveBeenCalledOnce());
    expect(stopped).toBe(1);
    expect(await screen.findByRole("heading", { name: "Kept." })).toBeInTheDocument();
  });

  it("record.is_wav: the phone's own recorder makes a 16 kHz mono 16-bit WAV, whatever the microphone", async () => {
    installFakeAudio(48000);
    const track = { stop: vi.fn() };
    vi.stubGlobal("navigator", {
      mediaDevices: { getUserMedia: async () => ({ getTracks: () => [track] }) },
    });
    try {
      const recording = await browserRecorder(() => undefined);
      speak(1000, 0.5);
      const blob = await recording.stop();
      expect(blob?.type).toBe("audio/wav");
      const bytes = new Uint8Array(await (blob as Blob).arrayBuffer());
      const view = new DataView(bytes.buffer);
      const ascii = (at: number, n: number) => String.fromCharCode(...bytes.subarray(at, at + n));
      expect(ascii(0, 4)).toBe("RIFF");
      expect(ascii(8, 4)).toBe("WAVE");
      expect(ascii(12, 4)).toBe("fmt ");
      expect(view.getUint16(20, true)).toBe(1);
      expect(view.getUint16(22, true)).toBe(1);
      expect(view.getUint32(24, true)).toBe(16000);
      expect(view.getUint16(34, true)).toBe(16);
      expect(ascii(36, 4)).toBe("data");
      expect(view.getUint32(40, true)).toBe(bytes.length - 44);
      expect(view.getUint32(4, true)).toBe(bytes.length - 8);
      expect(Math.abs((bytes.length - 44) / 2 - 16000)).toBeLessThanOrEqual(1);
      expect(track.stop).toHaveBeenCalled();
      expect(FakeAudioContext.last?.closed).toBe(true);
      // No sound heard: nothing to keep.
      const quiet = await browserRecorder(() => undefined);
      expect(await quiet.stop()).toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("recording with no microphone says so", async () => {
    render(
      <Keep
        engine={engineWith()}
        queue={memoryQueue()}
        connection={{
          kind: "connected",
          hello: {
            computerName: "Home computer",
            deviceName: "Pixel",
            connection: "simple",
            version: "0.1.0",
            minPhoneBuild: 1,
          },
        }}
        deviceName="Pixel"
        now={() => Date.now()}
        onHome={() => undefined}
        onQueued={() => undefined}
        recorder={async () => {
          throw new Error("none");
        }}
      />,
    );
    await userEvent.click(button("Record"));
    expect(
      await screen.findByText("Recording isn’t available in this browser."),
    ).toBeInTheDocument();
  });
});

function AskHarness({
  engine,
  onOpen = () => undefined,
}: {
  engine: PhoneEngine;
  onOpen?: () => void;
}) {
  const ask = useAskSession(engine);
  return (
    <Ask
      model={ask.model}
      connected
      onAsk={(q) => void ask.ask(q)}
      onStop={() => void ask.stop()}
      onOpen={onOpen}
      onBack={() => undefined}
    />
  );
}

describe("ask", () => {
  const question = async (q: string) => {
    fireEvent.change(screen.getByLabelText("Your question"), { target: { value: q } });
    await userEvent.click(button("Ask"));
  };

  it("ask.streams_then_receipts", async () => {
    render(<AskHarness engine={engineWith({ pace: 20 })} />);
    expect(screen.getByText("Ask about anything you’ve kept.")).toBeInTheDocument();
    await question("What do I need for Leo’s school trip?");
    expect(
      screen.getByText("Where this came from shows when the answer ends."),
    ).toBeInTheDocument();
    expect(await screen.findByText("From 2 things you kept")).toBeInTheDocument();
    expect(screen.getByText("Friday 21 November")).toBeInTheDocument();
    expect(button(/Year 4 trip letter/)).toBeInTheDocument();
    expect(button(/Class WhatsApp, 3 messages/)).toBeInTheDocument();
    expect(screen.getByText("Something wrong? Correct it on your computer.")).toBeInTheDocument();
  });

  it("ask.unsure_shows_both", async () => {
    render(<AskHarness engine={engineWith({ pace: 5 })} />);
    await question("When is the dentist?");
    expect(await screen.findByText("Tuesday 18 Nov, 14:00")).toBeInTheDocument();
    expect(screen.getByText("Thursday 20 Nov, 10:00")).toBeInTheDocument();
    expect(screen.getByText(/Not sure\./)).toBeInTheDocument();
    expect(
      screen.getByText("Choose which is right on your computer and Nocturne will remember."),
    ).toBeInTheDocument();
  });

  it("ask.too_many_questions", async () => {
    render(<AskHarness engine={engineWith()} />);
    await question("too many");
    expect(await screen.findByText(/That’s a lot of questions at once/)).toBeInTheDocument();
  });

  it("a computer that is not ready, or paused, is said plainly", async () => {
    const reply = (code: "notReady" | "paused") =>
      engineWith({}, { ask: async () => Promise.reject(new PhoneRequestError(code)) });
    render(<AskHarness engine={reply("notReady")} />);
    await question("hello");
    expect(await screen.findByText(/Nocturne is getting ready/)).toBeInTheDocument();
    cleanup();
    render(<AskHarness engine={reply("paused")} />);
    await question("hello");
    expect(await screen.findByText(/paused on your computer/)).toBeInTheDocument();
  });

  it("ask.stop_cancels: Stop sends a cancel and ends the answer", async () => {
    const cancelAsk = vi.fn(async () => undefined);
    render(<AskHarness engine={engineWith({ pace: 300 }, { cancelAsk })} />);
    await question("What do I need for Leo’s school trip?");
    await userEvent.click(await screen.findByRole("button", { name: "Stop" }));
    expect(await screen.findByText("Stopped.")).toBeInTheDocument();
    await waitFor(() => expect(cancelAsk).toHaveBeenCalledWith("ask-1"));
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
  });

  it("ask.question_capped: at most 2 KB, cut at a character", async () => {
    const long = "é".repeat(1500);
    const capped = capQuestion(long);
    expect(new TextEncoder().encode(capped).length).toBeLessThanOrEqual(2048);
    expect(capped).toBe("é".repeat(1024));
    expect(capQuestion("short")).toBe("short");
    render(<AskHarness engine={engineWith()} />);
    const input = screen.getByLabelText("Your question") as HTMLInputElement;
    fireEvent.change(input, { target: { value: long } });
    expect(new TextEncoder().encode(input.value).length).toBeLessThanOrEqual(2048);
  });

  it("there is no way to ask by speaking", () => {
    render(<AskHarness engine={engineWith()} />);
    expect(screen.queryByText(/speak/i)).not.toBeInTheDocument();
  });
});

describe("originals", () => {
  it("original.blob_url_revoked_on_leave", async () => {
    const made: string[] = [];
    const revoked: string[] = [];
    vi.spyOn(URL, "createObjectURL").mockImplementation(() => {
      const u = `blob:test/${made.length}`;
      made.push(u);
      return u;
    });
    vi.spyOn(URL, "revokeObjectURL").mockImplementation((u) => void revoked.push(u));
    const view = render(
      <Original engine={engineWith()} itemId="item-letter" quote={null} onBack={() => undefined} />,
    );
    expect(await screen.findByRole("img", { name: "Year 4 trip letter" })).toBeInTheDocument();
    expect(screen.getByText(/nothing is saved on this phone/)).toBeInTheDocument();
    expect(made).toHaveLength(1);
    expect(revoked).toEqual([]);
    view.unmount();
    expect(revoked).toEqual(made);
  });

  it("a note shows the words the answer used, marked", async () => {
    render(
      <Original
        engine={engineWith()}
        itemId="item-chat"
        quote="Packed lunch in a bag they can throw away please!"
        onBack={() => undefined}
      />,
    );
    const mark = await screen.findByText("Packed lunch in a bag they can throw away please!");
    expect(mark.tagName).toBe("MARK");
    expect(screen.getByText("The highlighted part is what the answer used.")).toBeInTheDocument();
  });

  it("an original whose type could run script is never given a URL or drawn", async () => {
    const create = vi.spyOn(URL, "createObjectURL");
    for (const type of ["text/html", "image/svg+xml", "application/xml"]) {
      const engine = engineWith(
        {},
        { original: async () => new Blob(["<svg onload=alert(1)>"], { type }) },
      );
      render(
        <Original engine={engine} itemId="item-letter" quote={null} onBack={() => undefined} />,
      );
      expect(await screen.findByText(/can’t be shown on the phone/)).toBeInTheDocument();
      expect(
        document.querySelector("img, iframe, embed, object, audio, a[href^='blob:']"),
      ).toBeNull();
      cleanup();
    }
    expect(create).not.toHaveBeenCalled();
  });

  it("original.pdf_offers_open_and_download, and falls back to the download when opening fails", async () => {
    const pdf = new Blob(["%PDF-1.7\n"], { type: "application/pdf" });
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test/pdf");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    const engine = engineWith(
      {},
      {
        getItem: async () => ({
          id: "item-doc",
          kind: "document",
          title: "Trip letter",
          keptAt: "2026-11-11T10:00:00.000Z",
          durationMs: null,
          pages: 2,
          hasOriginal: true,
          summary: null,
          text: null,
          truncated: false,
        }),
        original: async () => pdf,
      },
    );
    render(<Original engine={engine} itemId="item-doc" quote={null} onBack={() => undefined} />);
    const download = await screen.findByRole("link", { name: "Save a copy on this phone" });
    expect(download).toHaveAttribute("download", "Trip letter.pdf");
    expect(download).toHaveAttribute("href", "blob:test/pdf");
    expect(
      screen.getByText(/Viewing it saves nothing on this phone\. Saving a copy does\./),
    ).toBeInTheDocument();
    await userEvent.click(button("Open the document"));
    expect(open).toHaveBeenCalledWith("blob:test/pdf", "_blank");
    expect(click).toHaveBeenCalledOnce();
  });

  it("original.heic_is_a_download: it is never drawn as a picture", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test/heic");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    const heic = new Blob(
      [Uint8Array.of(0, 0, 0, 24, 0x66, 0x74, 0x79, 0x70, 0x68, 0x65, 0x69, 0x63)],
      { type: "image/heic" },
    );
    render(
      <Original
        engine={engineWith({}, { original: async () => heic })}
        itemId="item-letter"
        quote={null}
        onBack={() => undefined}
      />,
    );
    expect(await screen.findByText(/can’t be shown on this phone/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Save a copy on this phone" })).toHaveAttribute(
      "download",
      "Year 4 trip letter.heic",
    );
    expect(document.querySelector("img")).toBeNull();
  });

  it("a private or missing item says so, and a failure offers to try again", async () => {
    render(
      <Original
        engine={engineWith()}
        itemId="item-private"
        quote={null}
        onBack={() => undefined}
      />,
    );
    expect(await screen.findByText("That one stays on your computer.")).toBeInTheDocument();
    cleanup();
    render(
      <Original
        engine={engineWith()}
        itemId="item-nowhere"
        quote={null}
        onBack={() => undefined}
      />,
    );
    expect(await screen.findByText("That one is no longer kept.")).toBeInTheDocument();
    cleanup();
    const failing = engineWith(
      {},
      { getItem: async () => Promise.reject(new PhoneRequestError("busy")) },
    );
    render(
      <Original engine={failing} itemId="item-letter" quote={null} onBack={() => undefined} />,
    );
    expect(await screen.findByRole("button", { name: "Try again" })).toBeInTheDocument();
  });
});

describe("today’s page", () => {
  it("page.phone_edition_label", async () => {
    render(<PageScreen engine={engineWith()} onOpen={() => undefined} onBack={() => undefined} />);
    expect(
      await screen.findByRole("heading", { name: "Two things need you today." }),
    ).toBeInTheDocument();
    expect(screen.getByText("Phone edition · private lines left out")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Needs you · 2" })).toBeInTheDocument();
  });

  it("page.earlier_seven: no more than seven earlier days are offered", async () => {
    const days = Array.from({ length: 9 }, (_, i) => ({
      date: `2026-11-${String(12 - i).padStart(2, "0")}`,
      pageNumber: 40 - i,
      headline: "A day.",
      needsYou: 0,
    }));
    render(
      <PageScreen
        engine={engineWith({}, { listPages: async () => days })}
        onOpen={() => undefined}
        onBack={() => undefined}
      />,
    );
    const nav = await screen.findByRole("navigation", { name: "Earlier pages" });
    expect(within(nav).getAllByRole("button")).toHaveLength(7);
    expect(within(nav).getByRole("button", { name: "Thu 12" })).toBeInTheDocument();
  });

  it("page.none_yet", async () => {
    render(
      <PageScreen
        engine={engineWith({ noPage: true })}
        onOpen={() => undefined}
        onBack={() => undefined}
      />,
    );
    expect(await screen.findByRole("heading", { name: "No page yet." })).toBeInTheDocument();
  });

  it("a source opens the original", async () => {
    const opened: string[] = [];
    render(
      <PageScreen
        engine={engineWith()}
        onOpen={(r) => opened.push(r.itemId)}
        onBack={() => undefined}
      />,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: /Message you pasted · Thu 12 Nov/ }),
    );
    expect(opened).toEqual(["item-chat"]);
  });
});
