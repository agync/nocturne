import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PhoneKept, Receipt } from "../engine/bindings";
import { FakeAudioContext, installFakeAudio, speak } from "../test/fakeAudio";
import { chipsIn, sentence } from "../test/sentence";
import { App } from "./App";
import { type PhoneEngine, PhoneRequestError } from "./engine";
import { createMockPhoneEngine, type MockScript } from "./mock";
import { memoryQueue } from "./queue";
import { Ask, capQuestion, useAskSession } from "./screens/Ask";
import { Scan, Welcome } from "./screens/Entry";
import { browserRecorder, Keep } from "./screens/Keep";
import { LookBack } from "./screens/LookBack";
import { Original } from "./screens/Original";
import { PageScreen } from "./screens/PageScreen";
import { Which } from "./screens/Which";
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
    const where = screen.getByText(
      sentence("On your computer, open Nocturne and choose Your phones need the new address."),
    );
    expect(chipsIn(where)).toEqual(["Your phones need the new address"]);
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
  it("welcome.names_the_way_in: Settings, Away from home, then Add a phone", () => {
    render(<Welcome waiting={0} onScan={vi.fn()} onType={vi.fn()} onClear={vi.fn()} />);
    const body = screen.getByText(
      sentence(
        "On your computer, open Nocturne and choose Settings, then Away from home, then Add a phone. Scan the code with this phone, or type it here.",
      ),
    );
    expect(chipsIn(body)).toEqual(["Settings", "Away from home", "Add a phone"]);
  });

  it.each([
    ["pair", "Point at the code on your computer’s screen, under Add a phone.", ["Add a phone"]],
    [
      "relink",
      "Point at the code on your computer’s screen. It’s under Your phones need the new address.",
      ["Your phones need the new address"],
    ],
  ] as const)("scan.hint_names_the_place: %s", (purpose, text, chips) => {
    const env = { reader: vi.fn() } as never;
    render(
      <Scan
        purpose={purpose}
        env={env}
        accept={() => false}
        onSwitch={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(chipsIn(screen.getByText(sentence(text)))).toEqual(chips);
  });

  it("pair.name_then_words_then_allowed", async () => {
    await open("s=pair");
    const heading = await screen.findByRole("heading", { name: "Name this phone" });
    await waitFor(() => expect(heading).toHaveFocus());
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
    const choose = screen.getByText(
      sentence(
        "If they match, choose Allow on your computer. If they don’t, choose Don’t allow there and start again.",
      ),
    );
    expect(chipsIn(choose)).toEqual(["Allow", "Don’t allow"]);
    expect(choose.querySelector("strong")).toBeNull();
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
    const how = screen.getByText(
      sentence(
        "This page can’t reconnect a phone by itself. Open Nocturne from the home screen, choose Scan the new code, then point it at your computer.",
      ),
    );
    expect(chipsIn(how)).toEqual(["Scan the new code"]);
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

  it("phone.ask.online_mark", async () => {
    render(<AskHarness engine={engineWith({ pace: 5 })} />);
    await question("Trip online please");
    expect(
      await screen.findByText("Answered online by your provider (api.example.test)"),
    ).toBeInTheDocument();
    const where = screen.getByText(
      sentence("What was sent is listed on your computer. Choose Privacy, then See what was sent."),
    );
    expect(chipsIn(where)).toEqual(["Privacy", "See what was sent"]);
  });

  it("phone.ask.computer_mark", async () => {
    render(<AskHarness engine={engineWith({ pace: 5 })} />);
    await question("Trip on my computer");
    expect(await screen.findByText("Answered on your computer")).toBeInTheDocument();
    expect(screen.queryByText(/listed on your computer/)).not.toBeInTheDocument();
  });

  it("phone.ask.no_mark_when_off", async () => {
    render(<AskHarness engine={engineWith({ pace: 5 })} />);
    await question("What do I need for Leo’s school trip?");
    expect(await screen.findByText("From 2 things you kept")).toBeInTheDocument();
    expect(screen.queryByText(/Answered (online|on your)/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Online answers/)).not.toBeInTheDocument();
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

interface PageOptions {
  queue?: ReturnType<typeof memoryQueue>;
  connected?: boolean;
  onOpen?: (r: Receipt) => void;
  onJournal?: (day: string | null) => void;
}

function renderPage(engine: PhoneEngine, o: PageOptions = {}) {
  return render(
    <PageScreen
      engine={engine}
      queue={o.queue ?? memoryQueue()}
      connected={o.connected ?? true}
      now={() => Date.parse("2026-11-13T14:05:00")}
      onQueued={() => undefined}
      onOpen={o.onOpen ?? (() => undefined)}
      onJournal={o.onJournal ?? (() => undefined)}
      onBack={() => undefined}
    />,
  );
}

describe("today’s page", () => {
  it("page.phone_edition_label", async () => {
    renderPage(engineWith());
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
    renderPage(engineWith({}, { listPages: async () => days }));
    const nav = await screen.findByRole("navigation", { name: "Earlier pages" });
    expect(within(nav).getAllByRole("button")).toHaveLength(7);
    expect(within(nav).getByRole("button", { name: "Thu 12" })).toBeInTheDocument();
  });

  it("page.none_yet", async () => {
    renderPage(engineWith({ noPage: true }));
    expect(await screen.findByRole("heading", { name: "No page yet." })).toBeInTheDocument();
  });

  it("a source opens the original", async () => {
    const opened: string[] = [];
    renderPage(engineWith(), { onOpen: (r) => opened.push(r.itemId) });
    await userEvent.click(
      await screen.findByRole("button", { name: /Message you pasted · Thu 12 Nov/ }),
    );
    expect(opened).toEqual(["item-chat"]);
  });
});

describe("the page’s question from the journal", () => {
  const question = () => screen.findByRole("region", { name: /Yesterday you wrote/ });

  it("asks in the computer’s words, keeps a choice, then a line, on today’s journal", async () => {
    const engine = engineWith();
    const answer = vi.spyOn(engine, "answerJournalQuestion");
    renderPage(engine);
    const box = await question();
    expect(within(box).getByText("One question · from your journal")).toBeInTheDocument();
    expect(box).toHaveTextContent(
      "Yesterday you wrote: “Parents’ evening at six. Not looking forward to it.” How did it go?",
    );
    for (const choice of ["Fine", "Not great", "Not yet"]) {
      expect(within(box).getByRole("button", { name: choice })).toBeInTheDocument();
    }
    await userEvent.click(within(box).getByRole("button", { name: "Not great" }));
    expect(await within(box).findByText("Not great. Kept on today’s journal.")).toBeInTheDocument();
    const field = within(box).getByLabelText("Anything to add? (optional)");
    expect(field).toHaveAttribute("placeholder", "A line, if you like.");
    await waitFor(() => expect(field).toHaveFocus());
    await userEvent.type(field, "Mrs Okafor is worried about his reading.");
    await userEvent.click(within(box).getByRole("button", { name: "Add" }));
    expect(
      await within(box).findByText("“Mrs Okafor is worried about his reading.” Added."),
    ).toBeInTheDocument();
    expect(within(box).queryByLabelText("Anything to add? (optional)")).toBeNull();
    expect(within(box).getByRole("button", { name: "Open today’s journal ›" })).toBeInTheDocument();
    const [first, second] = answer.mock.calls;
    expect(first?.slice(0, 2)).toEqual(["2026-11-13", "notGreat"]);
    expect(first?.[3]).toBeUndefined();
    expect(second?.[1]).toBe("notGreat");
    expect(second?.[3]).toBe("Mrs Okafor is worried about his reading.");
  });

  it("the receipt opens the day the block was kept on, and Open today’s journal opens today", async () => {
    const opened: Array<string | null> = [];
    renderPage(engineWith(), { onJournal: (day) => opened.push(day) });
    const box = await question();
    await userEvent.click(within(box).getByRole("button", { name: /^Entry · Thu 12 Nov · / }));
    await userEvent.click(within(box).getByRole("button", { name: "Fine" }));
    await userEvent.type(within(box).getByLabelText("Anything to add? (optional)"), "Fine.");
    await userEvent.click(within(box).getByRole("button", { name: "Add" }));
    await userEvent.click(
      await within(box).findByRole("button", { name: "Open today’s journal ›" }),
    );
    expect(opened).toEqual(["2026-11-12", null]);
  });

  it("shows the answer the computer kept first", async () => {
    renderPage(engineWith({ answeredElsewhere: true }));
    const box = await question();
    await userEvent.click(within(box).getByRole("button", { name: "Not great" }));
    expect(await within(box).findByText("Fine. Kept on today’s journal.")).toBeInTheDocument();
    expect(
      within(box).getByText("Already answered, on your computer or another phone."),
    ).toBeInTheDocument();
  });

  it("an answer while the computer is away waits on the phone, with its line, and goes across once", async () => {
    const queue = memoryQueue();
    const away = engineWith({ keepFails: true });
    const { unmount } = renderPage(away, { queue });
    const box = await question();
    await userEvent.click(within(box).getByRole("button", { name: "Not great" }));
    expect(await within(box).findByText("Not great. Waiting on this phone.")).toBeInTheDocument();
    expect(
      within(box).getByText(
        "It’s on this phone and goes across when your computer is back. You can leave this page.",
      ),
    ).toBeInTheDocument();
    await userEvent.type(within(box).getByLabelText("Anything to add? (optional)"), "Long queue.");
    await userEvent.click(within(box).getByRole("button", { name: "Add" }));
    expect(
      await within(box).findByText("“Long queue.” Waiting on this phone."),
    ).toBeInTheDocument();
    const waiting = await queue.list();
    expect(waiting).toHaveLength(1);
    expect(waiting[0]?.answer).toEqual({
      pageDate: "2026-11-13",
      choice: "notGreat",
      line: "Long queue.",
    });
    unmount();

    const back = engineWith();
    const answer = vi.spyOn(back, "answerJournalQuestion");
    const result = await queue.flush(back);
    expect(result.sent).toBe(1);
    await queue.flush(back);
    expect(answer).toHaveBeenCalledTimes(1);
    const page = await back.getPage();
    expect(page.question?.answered).toBe("notGreat");
    expect(page.question?.journal?.line).toBe("Long queue.");
  });

  it("a refused answer says so and keeps the choices", async () => {
    renderPage(engineWith({ answerRefused: true }));
    const box = await question();
    await userEvent.click(within(box).getByRole("button", { name: "Fine" }));
    expect(await within(box).findByRole("alert")).toHaveTextContent(
      "That didn’t go across. Nothing was kept. Try again in a moment.",
    );
    expect(within(box).getByRole("button", { name: "Fine" })).toBeInTheDocument();
  });

  it("on an earlier page the choices work and no line can be added", async () => {
    const later = [{ date: "2026-11-14", pageNumber: 43, headline: "Later.", needsYou: 0 }];
    renderPage(engineWith({}, { listPages: async () => later }));
    const box = await question();
    await screen.findByRole("navigation", { name: "Earlier pages" });
    await userEvent.click(within(box).getByRole("button", { name: "Not yet" }));
    expect(await within(box).findByText("Not yet. Kept on today’s journal.")).toBeInTheDocument();
    expect(within(box).queryByLabelText("Anything to add? (optional)")).toBeNull();
  });

  it("only a question from the journal shows", async () => {
    const base = await engineWith().getPage();
    const asked = base.question;
    if (!asked) throw new Error("the scripted page asks a question");
    const yesNo = { ...base, question: { ...asked, kind: "yesNo" as const, journal: undefined } };
    renderPage(engineWith({}, { getPage: async () => yesNo }));
    await screen.findByRole("heading", { name: "Two things need you today." });
    expect(screen.queryByText("One question · from your journal")).toBeNull();
  });

  it("the phone’s journal draws the answer as the computer does", async () => {
    await open("s=journalAnswer");
    const row = (await screen.findByText("Not great")).closest("li");
    expect(row).toHaveTextContent("Not great Mrs Okafor is worried about his reading.");
    expect(row).toHaveTextContent("About: “Parents’ evening at six. Not looking forward to it.”");
    expect(row).toHaveTextContent("from Pixel");
  });
});

describe("journal on the phone", () => {
  it("home.journal_tile: connected opens it, asleep greys it with its reason", async () => {
    await open("s=connected");
    expect(await screen.findByText("Connected to Home computer")).toBeInTheDocument();
    expect(button(/^Journal/)).not.toHaveAttribute("aria-disabled");
    expect(screen.getByText("Today so far, and looking back.")).toBeInTheDocument();
    expect(
      screen.getByText("A note, a photo or a voice note, a feeling or a good thing."),
    ).toBeInTheDocument();
    cleanup();
    await open("s=asleep");
    expect(
      await screen.findByText("Last connected 07:48 · tries again by itself"),
    ).toBeInTheDocument();
    expect(button(/^Journal/)).toHaveAttribute("aria-disabled", "true");
    expect(
      screen.getAllByText("Back when your computer is. This phone keeps no copy."),
    ).toHaveLength(2);
  });

  it("keep.feeling_and_good: the chosen part, word and the good-thing mark reach the engine", async () => {
    const keepText = vi.fn(async () => kept);
    await open("s=connected", (s) => {
      s.engineFor = () => engineWith({}, { keepText });
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    await userEvent.type(screen.getByLabelText("Type something to keep"), "Lunch with Priya");
    await userEvent.click(screen.getByRole("button", { name: "Lit up" }));
    expect(screen.getByText("A word for it")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "cheerful" }));
    await userEvent.click(screen.getByRole("switch", { name: "This is a good thing" }));
    await userEvent.click(button("Keep"));
    await waitFor(() => expect(keepText).toHaveBeenCalledOnce());
    const [, , marks] = keepText.mock.calls[0] as unknown as [
      string,
      string,
      { feeling: { part: string; point: object; words: string[] }; good: boolean },
    ];
    expect(marks.feeling).toEqual({
      part: "litUp",
      point: { pleasant: 0.5, energy: 0.5 },
      words: ["cheerful"],
    });
    expect(marks.good).toBe(true);
    expect(
      await screen.findByRole("heading", { name: "Kept, with cheerful." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("It’s on today’s journal. Your computer reads it shortly."),
    ).toBeInTheDocument();
  });

  it("keep.feeling_waits: a feeling kept while asleep waits with the keep", async () => {
    const services = await open("s=asleep");
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    await userEvent.type(screen.getByLabelText("Type something to keep"), "Quiet evening");
    await userEvent.click(screen.getByRole("button", { name: "Settled" }));
    await userEvent.click(button("Keep"));
    expect(await screen.findByRole("heading", { name: "Waiting." })).toBeInTheDocument();
    const waiting = (await services.queue.list()).find((k) => k.text === "Quiet evening");
    expect(waiting?.feeling?.part).toBe("settled");
  });

  it("checkin.field_words_save: a part opens the words, up to three, and Save keeps it once", async () => {
    const keepCheckIn = vi.fn(createMockPhoneEngine({}).keepCheckIn);
    await open("s=checkIn", (s) => {
      s.engineFor = () => engineWith({}, { keepCheckIn });
    });
    expect(
      await screen.findByText("Tap or drag to a spot, then pick the words that fit."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    const sheet = await screen.findByRole("dialog", { name: "Words for Settled" });
    expect(within(sheet).getByText("Up to three")).toBeInTheDocument();
    await userEvent.click(within(sheet).getByRole("button", { name: "relieved" }));
    expect(within(sheet).getByText(/A worry has lifted/)).toBeInTheDocument();
    await userEvent.click(within(sheet).getByRole("button", { name: "cosy" }));
    await userEvent.click(within(sheet).getByRole("button", { name: "safe" }));
    await userEvent.click(within(sheet).getByRole("button", { name: "loved" }));
    expect(within(sheet).getByRole("button", { name: "loved" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    await userEvent.type(
      within(sheet).getByLabelText("What’s going on? (optional)"),
      "Gran’s scan",
    );
    await userEvent.click(within(sheet).getByRole("button", { name: "Save" }));
    expect(
      await screen.findByRole("heading", { name: "Saved. Relieved, cosy, safe." }),
    ).toBeInTheDocument();
    expect(screen.getByText("It’s on today’s journal.")).toBeInTheDocument();
    expect(keepCheckIn).toHaveBeenCalledOnce();
    const [input, id] = keepCheckIn.mock.calls[0] as unknown as [
      { feeling: { part: string; words: string[] }; line: string },
      string,
    ];
    expect(input.feeling.part).toBe("settled");
    expect(input.line).toBe("Gran’s scan");
    expect(id).toMatch(/^[A-Za-z0-9_-]{1,36}$/);
  });

  it("checkin.list_mode: every word as a list, with the meaning once chosen", async () => {
    const keepCheckIn = vi.fn(createMockPhoneEngine({}).keepCheckIn);
    await open("s=checkIn", (s) => {
      s.engineFor = () => engineWith({}, { keepCheckIn });
    });
    await userEvent.click(await screen.findByRole("button", { name: "Show all words as a list" }));
    expect(screen.getByRole("button", { name: "Save" })).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(screen.getByRole("checkbox", { name: "drained" }));
    expect(screen.getByText("Nothing left to give")).toBeInTheDocument();
    await userEvent.click(button("Save"));
    expect(await screen.findByRole("heading", { name: "Saved. Drained." })).toBeInTheDocument();
    const [input] = keepCheckIn.mock.calls[0] as unknown as [{ feeling: { part: string } }];
    expect(input.feeling.part).toBe("low");
  });

  it("checkin.asleep_waits: a check-in kept while asleep goes to the queue", async () => {
    const services = await open("s=asleep");
    await userEvent.click(await screen.findByRole("button", { name: /^Keep/ }));
    window.history.pushState({ view: { name: "checkin" } }, "");
    window.dispatchEvent(new PopStateEvent("popstate", { state: { view: { name: "checkin" } } }));
    await userEvent.click(await screen.findByRole("button", { name: "Show all words as a list" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "calm" }));
    await userEvent.click(button("Save"));
    expect(await screen.findByRole("heading", { name: "Waiting." })).toBeInTheDocument();
    expect((await services.queue.list()).some((k) => k.kind === "checkin")).toBe(true);
  });

  it("day.timeline: the Sam card, the entry, the rows and the pinned buttons", async () => {
    await open("s=connected");
    await userEvent.click(await screen.findByRole("button", { name: /^Journal/ }));
    expect(await screen.findByText("Today · Thursday 19 November")).toBeInTheDocument();
    const card = screen.getByRole("button", { name: /Which Sam is this\?/ });
    expect(within(card).getByText("Needs you")).toBeInTheDocument();
    expect(within(card).getByText("Same name")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Today’s entry" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Read it all" }));
    expect(screen.getByText(/dropped it at the school office/)).toBeInTheDocument();
    expect(screen.getAllByText(/Leo forgot his PE kit again/).length).toBeGreaterThan(1);
    expect(screen.getByText("Low · tired")).toBeInTheDocument();
    expect(screen.getByText("Sam fixed the gate without being asked.")).toBeInTheDocument();
    expect(button("Reflect on today")).toBeInTheDocument();
    expect(button("← Wednesday")).toBeInTheDocument();
    await userEvent.click(button("Choose a day"));
    expect(screen.getByRole("dialog", { name: "Choose a day" })).toBeInTheDocument();
  });

  it("day.empty: nothing kept offers Keep something and Check in", async () => {
    await open("s=journalEmpty");
    expect(await screen.findByText("Nothing kept today yet.")).toBeInTheDocument();
    expect(
      screen.getByText("Anything you keep, here or on your computer, shows up here."),
    ).toBeInTheDocument();
    expect(button("Keep something")).toBeInTheDocument();
    expect(button("Check in")).toBeInTheDocument();
  });

  it("day.asleep: no copy is kept, and what is waiting is listed", async () => {
    await open("s=journalAsleep");
    expect(
      await screen.findByText(
        "The journal is back when your computer is. This phone keeps no copy. What you keep now waits here.",
      ),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: "Waiting on this phone" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Voice note, 0:48")).toBeInTheDocument();
    expect(screen.getAllByText("waiting on this phone")).toHaveLength(2);
    expect(screen.queryByText("Today’s entry")).not.toBeInTheDocument();
  });

  it("reflect.writing_then_written: Reflect on today shows the writing state, then the reflection", async () => {
    await open("s=connected&pace=150");
    await userEvent.click(await screen.findByRole("button", { name: /^Journal/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Reflect on today" }));
    expect(
      await screen.findByText("Your computer is looking back on your day…"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "This can take a minute. You can leave this page; it will be in your journal.",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "The day" })).toBeInTheDocument();
    expect(screen.getByText("lunch with Priya")).toBeInTheDocument();
  });

  it("reflect.polls_while_writing: it asks again until the computer has written", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let reads = 0;
    const base = createMockPhoneEngine({});
    try {
      await open("s=lookBackToday", (s) => {
        s.engineFor = () =>
          engineWith(
            {},
            {
              lookBack: async (p) => ({
                type: "writing",
                glance:
                  (await base.getLookBack(p)).type === "notYet"
                    ? {
                        period: p,
                        kept: [],
                        felt: [],
                        good: [],
                        with: [],
                        entryLines: [],
                        entryItemId: null,
                      }
                    : ({} as never),
              }),
              getLookBack: async (p) => {
                reads += 1;
                return reads < 2
                  ? {
                      type: "writing",
                      glance: {
                        period: p,
                        kept: [],
                        felt: [],
                        good: [],
                        with: [],
                        entryLines: [],
                        entryItemId: null,
                      },
                    }
                  : base.lookBack(p);
              },
            },
          );
      });
      expect(
        await screen.findByText("Your computer is looking back on your day…"),
      ).toBeInTheDocument();
      // Two polls: the first still finds it writing, the second finds it written.
      await vi.advanceTimersByTimeAsync(4500);
      await vi.advanceTimersByTimeAsync(4500);
      expect(reads).toBe(2);
      expect(await screen.findByRole("heading", { name: "The day" })).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("which.answer_files_and_shows_done: the choice is sent, and the answer says what was filed", async () => {
    const answerSameName = vi.fn(createMockPhoneEngine({}).answerSameName);
    await open("s=connected", (s) => {
      s.engineFor = () => engineWith({}, { answerSameName });
    });
    await userEvent.click(await screen.findByRole("button", { name: /^Journal/ }));
    await userEvent.click(await screen.findByRole("button", { name: /Which Sam is this\?/ }));
    expect(
      await screen.findByText("“I think I’ll go with her and Sam on Saturday.”"),
    ).toBeInTheDocument();
    expect(screen.getByText(/^From a note · today, \d{1,2}:20 [ap]m$/)).toBeInTheDocument();
    expect(button("This one")).toHaveAttribute("aria-disabled", "true");
    expect(
      screen.getByText("Only this one is filed. You can change it later on your computer."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: /from work/ }));
    await userEvent.click(button("This one"));
    expect(
      await screen.findByRole("heading", { name: "Filed under Sam, from work." }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Only this one. You can change it on your computer."),
    ).toBeInTheDocument();
    expect(answerSameName).toHaveBeenCalledWith("snq-0001", {
      type: "entity",
      entityId: "ent-sam-work",
    });
  });
});

describe("journal on the phone, hardening", () => {
  const yesterday = { kind: "day", start: "2026-11-18", end: "2026-11-18" } as const;
  const lookProps = (engine: PhoneEngine, write = false) => ({
    engine,
    period: yesterday,
    write,
    today: "2026-11-19",
    now: () => Date.now(),
    onStarted: () => {},
    onBack: () => {},
    onToday: () => {},
    onItem: () => {},
    onReceipt: () => {},
  });
  const writingView = (p: typeof yesterday) =>
    ({
      type: "writing",
      glance: {
        period: p,
        kept: [],
        felt: [],
        good: [],
        with: [],
        entryLines: [],
        entryItemId: null,
      },
    }) as never;

  it("which.failed_read: a failed read says so and tries again, it is not called answered", async () => {
    const base = createMockPhoneEngine({});
    let reads = 0;
    const engine = engineWith(
      {},
      {
        getJournalDay: async (d) => {
          reads += 1;
          if (reads === 1) throw new Error("down");
          return base.getJournalDay(d);
        },
      },
    );
    render(
      <Which
        engine={engine}
        question={null}
        questionId="snq-0001"
        day="2026-11-19"
        today
        onBack={() => {}}
        onDone={() => {}}
      />,
    );
    expect(
      await screen.findByText("Nocturne couldn’t get this day. What you kept is safe."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Nothing to answer here now.")).not.toBeInTheDocument();
    await userEvent.click(button("Try again"));
    expect(await screen.findByText("Which Sam is this?")).toBeInTheDocument();
  });

  it("which.gone: only a question that is truly gone says it was answered", async () => {
    const base = createMockPhoneEngine({});
    const engine = engineWith(
      {},
      { getJournalDay: async (d) => ({ ...(await base.getJournalDay(d)), sameName: [] }) },
    );
    render(
      <Which
        engine={engine}
        question={null}
        questionId="snq-0001"
        day="2026-11-19"
        today
        onBack={() => {}}
        onDone={() => {}}
      />,
    );
    expect(await screen.findByText("Nothing to answer here now.")).toBeInTheDocument();
    expect(
      screen.getByText("It was answered on your computer, or what it was about is gone."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back to today" })).toBeInTheDocument();
  });

  it("lookback.retry_read: Try again after a failed read reads, it does not ask to write", async () => {
    const base = createMockPhoneEngine({});
    let reads = 0;
    const lookBack = vi.fn(base.lookBack);
    const getLookBack = vi.fn(async (p: typeof yesterday) => {
      reads += 1;
      if (reads === 1) throw new Error("down");
      return base.getLookBack(p);
    });
    render(<LookBack {...lookProps(engineWith({}, { getLookBack, lookBack }))} />);
    expect(
      await screen.findByText("Nocturne couldn’t get this day. What you kept is safe."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/couldn’t write this/)).not.toBeInTheDocument();
    await userEvent.click(button("Try again"));
    await waitFor(() => expect(getLookBack).toHaveBeenCalledTimes(2));
    expect(lookBack).not.toHaveBeenCalled();
  });

  it("lookback.retry_write: Try again after a failed write asks to write again", async () => {
    const base = createMockPhoneEngine({});
    let writes = 0;
    const lookBack = vi.fn(async (p: typeof yesterday) => {
      writes += 1;
      if (writes === 1) throw new Error("down");
      return base.lookBack(p);
    });
    const getLookBack = vi.fn(base.getLookBack);
    render(<LookBack {...lookProps(engineWith({}, { getLookBack, lookBack }), true)} />);
    expect(
      await screen.findByText("Nocturne couldn’t write this just now. Your entries are safe."),
    ).toBeInTheDocument();
    await userEvent.click(button("Try again"));
    await waitFor(() => expect(lookBack).toHaveBeenCalledTimes(2));
    expect(getLookBack).not.toHaveBeenCalled();
  });

  it("lookback.polling_stops: leaving while it says writing stops the asking", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const getLookBack = vi.fn(async (p: typeof yesterday) => writingView(p));
      const view = render(<LookBack {...lookProps(engineWith({}, { getLookBack }))} />);
      expect(
        await screen.findByText("Your computer is looking back on your day…"),
      ).toBeInTheDocument();
      await vi.advanceTimersByTimeAsync(4100);
      const seen = getLookBack.mock.calls.length;
      expect(seen).toBeGreaterThan(1);
      view.unmount();
      await vi.advanceTimersByTimeAsync(20_000);
      expect(getLookBack).toHaveBeenCalledTimes(seen);
    } finally {
      vi.useRealTimers();
    }
  });

  it("lookback.polling_hidden: a hidden page does not ask", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const state = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    try {
      const getLookBack = vi.fn(async (p: typeof yesterday) => writingView(p));
      render(<LookBack {...lookProps(engineWith({}, { getLookBack }))} />);
      expect(
        await screen.findByText("Your computer is looking back on your day…"),
      ).toBeInTheDocument();
      await vi.advanceTimersByTimeAsync(13_000);
      expect(getLookBack).toHaveBeenCalledTimes(1);
      state.mockReturnValue("visible");
      await vi.advanceTimersByTimeAsync(4100);
      expect(getLookBack.mock.calls.length).toBeGreaterThan(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("checkin.double_save: two quick taps on Save keep one check-in", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const base = createMockPhoneEngine({});
    const keepCheckIn = vi.fn(async (...args: Parameters<PhoneEngine["keepCheckIn"]>) => {
      await gate;
      return base.keepCheckIn(...args);
    });
    const services = await open("s=checkIn", (s) => {
      s.engineFor = () => engineWith({}, { keepCheckIn });
    });
    await userEvent.click(await screen.findByRole("radio", { name: "Settled" }));
    const sheet = await screen.findByRole("dialog", { name: "Words for Settled" });
    await userEvent.click(within(sheet).getByRole("button", { name: "relieved" }));
    const save = within(sheet).getByRole("button", { name: "Save" });
    fireEvent.click(save);
    fireEvent.click(save);
    await waitFor(() => expect(keepCheckIn).toHaveBeenCalledTimes(1));
    expect((await services.queue.list()).filter((k) => k.kind === "checkin")).toHaveLength(1);
    release();
    expect(await screen.findByRole("heading", { name: /^Saved\./ })).toBeInTheDocument();
    expect(keepCheckIn).toHaveBeenCalledTimes(1);
  });

  it("checkin.radio_part: a part chosen by its radio opens the words with no drag", async () => {
    await open("s=checkIn");
    const settled = await screen.findByRole("radio", { name: "Settled" });
    await userEvent.click(settled);
    expect(settled).toBeChecked();
    expect(await screen.findByRole("dialog", { name: "Words for Settled" })).toBeInTheDocument();
  });

  it("checkin.sheet_focus_and_close: the sheet traps focus and closes on Escape and back", async () => {
    await open("s=checkIn");
    await userEvent.click(await screen.findByRole("radio", { name: "Settled" }));
    let sheet = await screen.findByRole("dialog", { name: "Words for Settled" });
    const stops = within(sheet).getAllByRole("button");
    const last = within(sheet).getByRole("button", { name: "Save" });
    last.focus();
    await userEvent.tab();
    expect(sheet.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).toBe(stops[0]);
    await userEvent.tab({ shift: true });
    expect(document.activeElement).toBe(last);
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await userEvent.click(screen.getByRole("radio", { name: "Settled" }));
    sheet = await screen.findByRole("dialog", { name: "Words for Settled" });
    window.history.back();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});

describe("journal on the phone, follow-up copy", () => {
  const yesterday = { kind: "day", start: "2026-11-18", end: "2026-11-18" } as const;
  const week = { kind: "week", start: "2026-11-09", end: "2026-11-15" } as const;
  const lookProps = (engine: PhoneEngine, period: typeof yesterday | typeof week = yesterday) => ({
    engine,
    period,
    write: false,
    today: "2026-11-19",
    now: () => Date.now(),
    onStarted: () => {},
    onBack: () => {},
    onToday: () => {},
    onItem: () => {},
    onReceipt: () => {},
  });

  it("opens your journal while looking back loads", () => {
    const getLookBack = vi.fn(() => new Promise<never>(() => {}));
    render(<LookBack {...lookProps(engineWith({}, { getLookBack }))} />);
    expect(screen.getByText("Opening your journal…")).toBeInTheDocument();
  });

  it("opens the question while a name question loads", () => {
    const getJournalDay = vi.fn(() => new Promise<never>(() => {}));
    render(
      <Which
        engine={engineWith({}, { getJournalDay })}
        question={null}
        questionId="snq-0001"
        day="2026-11-19"
        today
        onBack={() => {}}
        onDone={() => {}}
      />,
    );
    expect(screen.getByText("Opening the question…")).toBeInTheDocument();
  });

  it("writes the overnight footer with the computer first", async () => {
    render(<LookBack {...lookProps(engineWith())} />);
    expect(
      await screen.findByText(
        "Written overnight on your computer, from 7 things you kept that day.",
      ),
    ).toBeInTheDocument();
  });

  it("writes the on-demand footer without 'that day'", async () => {
    const base = createMockPhoneEngine({});
    const getLookBack = vi.fn(async (p: typeof yesterday) => {
      const view = await base.getLookBack(p);
      return view.type === "written"
        ? {
            ...view,
            reflection: { ...view.reflection, written: "onDemand" as const, fromCount: 1 },
          }
        : view;
    });
    render(<LookBack {...lookProps(engineWith({}, { getLookBack }))} />);
    expect(
      await screen.findByText(/^Written on your computer at .+, from 1 thing you kept\.$/),
    ).toBeInTheDocument();
  });

  it("says which days a name came up in, without guessing what was done", async () => {
    render(<LookBack {...lookProps(engineWith(), week)} />);
    expect(
      await screen.findByText(
        "On 3 of the 4 days with Gran in your journal, you checked in as settled.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a recap with the partial line while things are still being read", async () => {
    const base = createMockPhoneEngine({});
    const getLookBack = vi.fn(async (p: typeof week) => {
      const view = await base.getLookBack(p);
      return view.type === "recap" ? { ...view, recap: { ...view.recap, stillReading: 1 } } : view;
    });
    render(<LookBack {...lookProps(engineWith({}, { getLookBack }), week)} />);
    expect(
      await screen.findByText(
        "1 thing from this week is still being read, so it isn’t in this yet.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Highlights" })).toBeInTheDocument();
  });

  it("offers Look back on this week, without so far, for a week that is over", async () => {
    const getLookBack = vi.fn(async (p: typeof week) => ({
      type: "notYet" as const,
      glance: {
        period: p,
        kept: [],
        felt: [],
        good: [],
        with: [],
        entryLines: [],
        entryItemId: null,
      },
    }));
    render(<LookBack {...lookProps(engineWith({}, { getLookBack }), week)} />);
    expect(
      await screen.findByRole("button", { name: "Look back on this week" }),
    ).toBeInTheDocument();
  });

  it("asks for a line without promising to talk, and has no microphone", async () => {
    await open("s=checkIn");
    await userEvent.click(await screen.findByRole("button", { name: "Show all words as a list" }));
    expect(await screen.findByPlaceholderText("A line, if you like.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /microphone|voice|talk|record/i })).toBeNull();
  });
});
