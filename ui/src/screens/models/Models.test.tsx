import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EngineError } from "../../engine/client";
import type { ModelsScenario } from "../../engine/mock/models";
import { renderApp, setViewport } from "../../test/render";
import { modelsEngine } from "./testing";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

async function open(path: string, scenario: ModelsScenario = "standard") {
  const { engine, sim } = modelsEngine(scenario);
  renderApp({ hash: `#${path}`, engine });
  return { engine, sim, user: userEvent.setup() };
}

const row = (name: string) => {
  const item = screen.getByText(name, { selector: "span" }).closest("li");
  if (!item) throw new Error(`no row for ${name}`);
  return within(item);
};

describe("models", () => {
  it("models.tier_view: the starting point, the meter and the four jobs", async () => {
    await open("/settings/models");
    expect(await screen.findByRole("heading", { level: 1, name: "Models" })).toBeInTheDocument();
    expect(
      await screen.findByText(
        "Nocturne chose these for this computer’s 32 GB of memory. Most people never change them.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Standard · for this computer")).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "Up to 10.6 GB of 19.3 GB at the busiest" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Up to 10.6 GB of the 19.3 GB Nocturne allows itself"),
    ).toBeInTheDocument();
    expect(screen.getByText("Answering 8.1 GB")).toBeInTheDocument();
    expect(row("Qwen3.5-9B-GGUF").getByText("Recommended")).toBeInTheDocument();
    expect(row("kokoro-v1").getByText(/on its own/)).toBeInTheDocument();
    expect(screen.getAllByRole("link", { name: /^Change / })).toHaveLength(4);
  });

  it("models.custom_reset: names what changed and goes back to the tier", async () => {
    const { user } = await open("/settings/models", "custom");
    expect(
      await screen.findByText("You changed Answering questions. Everything else is as Standard."),
    ).toBeInTheDocument();
    expect(row("Qwen3-VL-8B").getByText("Not tested with Nocturne")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reset to Standard" }));
    expect(await screen.findByText("Standard · for this computer")).toBeInTheDocument();
    expect(screen.getByText("Qwen3.5-9B-GGUF", { selector: "span" })).toBeInTheDocument();
  });

  it("models.attention_row: a stopped download says so and offers a way on", async () => {
    const { user } = await open("/settings/models", "attention");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The download stopped at 2.1 GB of 6.2 GB.");
    expect(alert).toHaveTextContent("Nothing has changed. Qwen3.5-9B-GGUF keeps working.");
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("progressbar")).toBeInTheDocument();
  });
});

describe("picker", () => {
  it("picker.marks_not_colour_only: every mark has an icon and a word", async () => {
    await open("/settings/models/finding");
    await screen.findByText("Qwen3-Embedding-4B");
    for (const [name, word] of [
      ["Qwen3-Embedding-0.6B-GGUF", "Fits"],
      ["Qwen3-Embedding-4B", "Tight"],
      ["Qwen3-Embedding-8B", "Won’t fit"],
    ] as const) {
      const option = screen.getByText(name).closest("div");
      const mark = option?.querySelector("[data-fit]") as HTMLElement;
      expect(mark.textContent, name).toBe(word);
      expect(
        screen.getByRole("radio", { name: new RegExp(name) }),
        name,
      ).toHaveAccessibleDescription(new RegExp(`\\b${word}$|\\b${word} `));
      expect(mark.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    }
    expect(
      screen.getByText("Works, but Nocturne may slow down at its busiest."),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Qwen3-Embedding-8B/ })).toBeDisabled();
  });

  it("picker.why_reasons: one reason, opened in place with a way on", async () => {
    const { user } = await open("/settings/models/finding");
    await screen.findByText("Qwen3-Embedding-8B");
    expect(
      screen.getByText("Fits only if Answering questions uses a smaller one."),
    ).toBeInTheDocument();
    const why = screen.getByRole("button", { name: "Why?" });
    expect(why).toHaveAttribute("aria-expanded", "false");
    await user.click(why);
    expect(why).toHaveAttribute("aria-expanded", "true");
    const region = screen.getByRole("region", { name: "Why this can’t be chosen" });
    expect(region).toHaveTextContent("It would fit if Answering questions used a smaller one.");
    expect(
      within(region).getByRole("link", { name: "Change Answering questions…" }),
    ).toHaveAttribute("href", "#/settings/models/answering");
  });

  it("picker.online_only_in_answering: M9 hands off to the online setup", async () => {
    const { user } = await open("/settings/models/answering");
    await screen.findByText("Online answers");
    expect(
      screen.getByText(/Photos are still read on this computer, by Qwen3.5-9B-GGUF\./),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /Online answers/ }));
    const setup = await screen.findByRole("link", { name: "Set up online answers…" });
    expect(setup).toHaveAttribute("href", "#/settings/online/setup");
    expect(screen.getByRole("button", { name: "Use the chosen one…" })).toBeDisabled();
  });

  it("picker.online_only_in_answering: the other jobs have no online option", async () => {
    for (const job of ["recordings", "finding", "aloud"]) {
      window.location.hash = "";
      const { unmount } = renderApp({
        hash: `#/settings/models/${job}`,
        engine: modelsEngine().engine,
      });
      await screen.findByRole("heading", { level: 1 });
      await screen.findAllByRole("radio");
      expect(screen.queryByText("Online answers")).toBeNull();
      unmount();
    }
  });

  it("picker.loading: says what it is doing while the choices come", async () => {
    const { engine } = modelsEngine();
    engine.listModelChoices = () => new Promise(() => undefined);
    renderApp({ hash: "#/settings/models/answering", engine });
    expect(await screen.findByText("Checking what this computer can run…")).toBeInTheDocument();
  });

  it("picker.aloud_single_choice: one voice, informational, nothing to confirm", async () => {
    await open("/settings/models/aloud");
    await screen.findByText(/is the only voice Nocturne can use for now/);
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(1);
    expect(radios[0]).toBeDisabled();
    expect(radios[0]).toBeChecked();
    expect(screen.getByRole("button", { name: "Use the chosen one…" })).toBeDisabled();
  });
});

describe("confirm", () => {
  it("confirm.download_shows_size_and_space: asks before a download and starts it on yes", async () => {
    const { user, sim } = await open("/settings/models/answering");
    await user.click(await screen.findByRole("radio", { name: /Qwen3-VL-8B/ }));
    await user.click(screen.getByRole("button", { name: "Use the chosen one…" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Get Qwen3-VL-8B for answering questions?",
    });
    expect(dialog).toHaveTextContent("6.2 GB, once");
    expect(dialog).toHaveTextContent("220 GB now, about 214 GB after");
    expect(dialog).toHaveTextContent("Up to 7.4 GB, down from 8.1 GB · fits");
    expect(dialog).toHaveTextContent("Nocturne hasn’t been tested with it.");
    expect(within(dialog).getByRole("button", { name: "Not now" })).toHaveFocus();
    expect(sim.view().change).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Download 6.2 GB" }));
    await waitFor(() => expect(sim.view().change?.model).toBe("Qwen3-VL-8B"));
    expect(window.location.hash).toBe("#/settings/models");
    expect(
      await screen.findByRole("progressbar", { name: "Getting Qwen3-VL-8B" }),
    ).toBeInTheDocument();
  });

  it("confirm.quality_warning: a smaller one on the computer warns, downloads nothing", async () => {
    const { user, sim } = await open("/settings/models/answering");
    await user.click(await screen.findByRole("radio", { name: /Qwen3.5-4B-MTP/ }));
    await user.click(screen.getByRole("button", { name: "Use the chosen one…" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Use Qwen3.5-4B-MTP for answering questions?",
    });
    expect(dialog).toHaveTextContent("smaller than Qwen3.5-9B-GGUF");
    expect(dialog).toHaveTextContent("Photos are read by it too.");
    expect(dialog).toHaveTextContent("None. It’s already on this computer.");
    expect(dialog).toHaveTextContent("Up to 4.9 GB, down from 8.1 GB");
    await user.click(within(dialog).getByRole("button", { name: "Use Qwen3.5-4B-MTP" }));
    await waitFor(() => expect(sim.view().jobs[0]?.model).toBe("Qwen3.5-4B-MTP"));
  });

  it("confirm.reindex_preview: says it will re-read, for how long, and that nothing goes missing", async () => {
    const { user, sim } = await open("/settings/models/finding");
    await user.click(await screen.findByRole("radio", { name: /Qwen3-Embedding-4B/ }));
    await user.click(screen.getByRole("button", { name: "Use the chosen one…" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Use Qwen3-Embedding-4B for finding memories?",
    });
    expect(dialog).toHaveTextContent("re-read all 3,610 things you kept");
    expect(dialog).toHaveTextContent("so nothing goes missing");
    expect(dialog).toHaveTextContent("About 25 minutes, while this computer is on");
    expect(dialog).toHaveTextContent("tight");
    await user.click(within(dialog).getByRole("button", { name: "Download and re-read" }));
    await waitFor(() => expect(sim.view().change?.job).toBe("finding"));
  });

  it("confirm.recordings_apply_to_new_only", async () => {
    const { user } = await open("/settings/models/recordings");
    await user.click(await screen.findByRole("radio", { name: /Whisper-Large-v3-Turbo/ }));
    await user.click(screen.getByRole("button", { name: "Use the chosen one…" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("New recordings. What you kept before stays as it was read.");
    await user.click(within(dialog).getByRole("button", { name: "Keep Whisper-Small" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("download", () => {
  it("download.cancel_confirms: stopping asks first, and the current one is untouched", async () => {
    const { user, sim } = await open("/settings/models", "downloading");
    const bar = await screen.findByRole("progressbar", { name: "Getting Qwen3-VL-8B" });
    expect(bar).toHaveAttribute("aria-valuetext", expect.stringContaining("2.1 GB of 6.2 GB"));
    await user.click(screen.getByRole("button", { name: "Cancel…" }));
    const dialog = await screen.findByRole("dialog", { name: "Stop getting Qwen3-VL-8B?" });
    expect(dialog).toHaveTextContent("The 2.1 GB downloaded so far is removed.");
    await user.click(within(dialog).getByRole("button", { name: "Keep downloading" }));
    expect(sim.view().change).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Cancel…" }));
    await user.click(await screen.findByRole("button", { name: "Stop and remove" }));
    await waitFor(() => expect(sim.view().change).toBeNull());
    expect(sim.view().jobs[0]?.model).toBe("Qwen3.5-9B-GGUF");
  });

  it("download.pause_and_resume", async () => {
    const { user } = await open("/settings/models", "downloading");
    await user.click(await screen.findByRole("button", { name: "Pause" }));
    expect(
      await screen.findByRole("progressbar", { name: "Paused: getting Qwen3-VL-8B" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Resume" }));
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("download.resumed_after_restart", async () => {
    await open("/settings/models", "resumed");
    expect(
      await screen.findByRole("progressbar", { name: "Carrying on: getting Qwen3-VL-8B" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Nocturne restarted, so the download picked up where it left off/),
    ).toBeInTheDocument();
  });
});

describe("reindex", () => {
  it("reindex.cancel_keeps_old: cancelling leaves the old way in use", async () => {
    const { user, sim } = await open("/settings/models", "reindex");
    expect(
      await screen.findByRole("progressbar", {
        name: "Re-reading what you kept for Qwen3-Embedding-4B",
      }),
    ).toHaveAttribute("aria-valuetext", expect.stringContaining("1,240 of 3,610"));
    await user.click(screen.getByRole("button", { name: "Cancel, keep the old way" }));
    expect(
      await screen.findByText(
        "Stopped. Finding memories still uses Qwen3-Embedding-0.6B-GGUF, as before. Nothing was lost.",
      ),
    ).toBeInTheDocument();
    expect(sim.view().jobs[3]?.model).toBe("Qwen3-Embedding-0.6B-GGUF");
  });

  it("reindex.done_offers_removal", async () => {
    await open("/settings/models", "reindexDone");
    expect(
      await screen.findByText(
        "Finished. Finding memories now uses Qwen3-Embedding-4B for all 3,610 things you kept.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Remove Qwen3-Embedding-0.6B-GGUF, free 0.64 GB" }),
    ).toBeInTheDocument();
  });
});

describe("old", () => {
  it("old.keep_is_default: Keep comes first, nothing is removed unless chosen, focus goes to the heading", async () => {
    const { user, sim } = await open("/settings/models", "oldKeep");
    const keep = await screen.findByRole("button", { name: "Keep it" });
    expect(keep).not.toHaveFocus();
    const buttons = within(keep.parentElement as HTMLElement).getAllByRole("button");
    expect(buttons[0]).toBe(keep);
    expect(screen.getByText("Qwen3-VL-8B is now answering questions.")).toBeInTheDocument();
    await user.click(keep);
    await waitFor(() => expect(sim.view().old).toBeNull());
    expect(screen.getByRole("heading", { level: 1, name: "Models" })).toHaveFocus();
    expect((await sim.choices("answering")).find((c) => c.id === "Qwen3.5-9B-GGUF")?.onDisk).toBe(
      true,
    );
  });

  it("old.remove_frees_space", async () => {
    const { user } = await open("/settings/models", "oldKeep");
    await user.click(await screen.findByRole("button", { name: "Remove, free 6.9 GB" }));
    expect(
      await screen.findByText(
        "Removed Qwen3.5-9B-GGUF and freed 6.9 GB. You can get it again from this list at any time.",
      ),
    ).toBeInTheDocument();
  });
});

describe("picker, more", () => {
  it("picker.current_stays_listed: the current one is listed and checked even when it no longer fits", async () => {
    const { engine } = modelsEngine();
    const real = engine.listModelChoices;
    engine.listModelChoices = async (job) =>
      (await real(job)).map((c) =>
        c.current ? { ...c, fit: "wontFit", reason: { type: "memory", limiter: null } } : c,
      );
    renderApp({ hash: "#/settings/models/answering", engine });
    const radio = await screen.findByRole("radio", { name: /Qwen3.5-9B-GGUF/ });
    expect(radio).toBeChecked();
    expect(radio).not.toBeDisabled();
    expect(radio).toHaveAccessibleDescription(/Won’t fit/);
    expect(radio).toHaveAccessibleDescription(/Too much memory beside the other jobs\./);
    expect(screen.getAllByText("Current").length).toBeGreaterThan(0);
  });

  it("picker.preview_names_what_it_previews: nothing can be picked while it loads, and the confirm changes what it names", async () => {
    const { engine, sim } = modelsEngine();
    let release: () => void = () => undefined;
    const real = engine.previewModelChange;
    engine.previewModelChange = (job, model) =>
      new Promise((resolve) => {
        release = () => resolve(real(job, model));
      });
    renderApp({ hash: "#/settings/models/answering", engine });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("radio", { name: /Qwen3-VL-8B/ }));
    await user.click(screen.getByRole("button", { name: "Use the chosen one…" }));
    await waitFor(() => expect(screen.getByRole("radio", { name: /Gemma-4-E4B/ })).toBeDisabled());
    expect(screen.getByRole("button", { name: "Use the chosen one…" })).toBeDisabled();
    release();
    const dialog = await screen.findByRole("dialog", { name: /Get Qwen3-VL-8B/ });
    await user.click(within(dialog).getByRole("button", { name: "Download 6.2 GB" }));
    await waitFor(() => expect(sim.view().change?.model).toBe("Qwen3-VL-8B"));
  });

  it("picker.read_error_retries: a failed read says so and tries again", async () => {
    const { engine } = modelsEngine();
    const real = engine.listModelChoices;
    let fail = true;
    engine.listModelChoices = async (job) => {
      if (fail) throw new Error("no");
      return real(job);
    };
    renderApp({ hash: "#/settings/models/finding", engine });
    const user = userEvent.setup();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t check what this computer can run.");
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("radio", { name: /Qwen3-Embedding-4B/ })).toBeInTheDocument();
  });

  it("picker.use_waits_for_a_running_change: Use is off and points to the running card", async () => {
    await open("/settings/models/answering", "downloading");
    await user_wait();
    expect(screen.getByRole("button", { name: "Use the chosen one…" })).toBeDisabled();
    expect(screen.getByRole("link", { name: "See where it is" })).toHaveAttribute(
      "href",
      "#/settings/models",
    );
  });
});

describe("models, page errors and the row", () => {
  it("models.read_error_retries", async () => {
    const { engine } = modelsEngine();
    const real = engine.getModels;
    let fail = true;
    engine.getModels = async () => {
      if (fail) throw new Error("no");
      return real();
    };
    renderApp({ hash: "#/settings/models", engine });
    const user = userEvent.setup();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Nocturne couldn’t read these details. What you kept is safe.");
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Standard · for this computer")).toBeInTheDocument();
  });

  it("models.unavailable: says what is unavailable and what went wrong", async () => {
    const { engine } = modelsEngine();
    engine.getModels = async () => {
      throw new EngineError("unavailable", "this is not available in a browser");
    };
    renderApp({ hash: "#/settings/models", engine });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Choosing models isn’t available here.");
    expect(alert).toHaveTextContent("What went wrong: this is not available in a browser");
  });

  it("models.attention_row_in_settings: the Settings row says Needs you and why", async () => {
    await open("/settings", "attention");
    const link = await screen.findByRole("link", {
      name: "Choose what runs each job, Needs you. Open",
    });
    expect(link).toHaveAccessibleDescription("A download stopped. What you use now keeps working.");
  });

  it("download.cancel_moves_focus_to_the_heading", async () => {
    const { user } = await open("/settings/models", "downloading");
    await user.click(await screen.findByRole("button", { name: "Cancel…" }));
    await user.click(await screen.findByRole("button", { name: "Stop and remove" }));
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1, name: "Models" })).toHaveFocus(),
    );
  });

  it("models.progress_announces_phases_not_amounts: the live region holds no numbers", async () => {
    await open("/settings/models", "downloading");
    const bar = await screen.findByRole("progressbar", { name: "Getting Qwen3-VL-8B" });
    for (const region of screen.getAllByRole("status")) {
      expect(region.textContent).not.toMatch(/\d+(\.\d+)? GB/);
    }
    expect(bar).toHaveAttribute("aria-valuetext", expect.stringContaining("2.1 GB of 6.2 GB"));
  });
});

const user_wait = () => screen.findByRole("link", { name: "See where it is" });
