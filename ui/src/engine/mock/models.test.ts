import { describe, expect, it } from "vitest";
import { ModelsSim } from "./models";

const clock = () => {
  let now = 1_000;
  return {
    now: () => now,
    tick: (ms: number) => {
      now += ms;
    },
  };
};

describe("mock.models", () => {
  it("mock.models.preset_view: the standard tier with all four jobs and its memory", () => {
    const view = new ModelsSim("standard", "standard").view();
    expect(view.preset).toBe("standard");
    expect(view.custom).toBe(false);
    expect(view.jobs.map((j) => j.job)).toEqual(["answering", "recordings", "aloud", "finding"]);
    expect(view.busiestBytes).toBeCloseTo(10.6e9, -8);
    expect(view.budgetBytes).toBeGreaterThan(view.busiestBytes);
    expect(view.change).toBeNull();
    expect(view.attention).toBe(false);
  });

  it("mock.models.fit_marks: every fit is on offer, and greyed ones carry a reason", async () => {
    const sim = new ModelsSim("standard", "standard");
    const all = [...(await sim.choices("answering")), ...(await sim.choices("finding"))];
    expect(new Set(all.map((c) => c.fit))).toEqual(
      new Set(["fits", "tight", "wontFit", "cantUse"]),
    );
    for (const c of all) {
      if (c.fit === "wontFit" || c.fit === "cantUse") expect(c.reason, c.name).not.toBeNull();
    }
    expect(all.some((c) => c.id.startsWith("user."))).toBe(false);
  });

  it("mock.models.download_needs_yes: a model not on the computer is fetched only with consent", async () => {
    const sim = new ModelsSim("standard", "standard");
    await expect(sim.change("answering", "Qwen3-VL-8B", false)).rejects.toMatchObject({
      code: "needsConsent",
    });
    expect(sim.view().change).toBeNull();
  });

  it("mock.models.download_keeps_old_until_ready: the current model works until the new one lands", async () => {
    const c = clock();
    const sim = new ModelsSim("standard", "standard", c.now);
    const started = await sim.change("answering", "Qwen3-VL-8B", true);
    expect(started.change?.phase.type).toBe("downloading");
    c.tick(2000);
    const half = sim.view();
    expect(half.jobs[0]?.model).toBe("Qwen3.5-9B-GGUF");
    expect(half.change?.phase).toMatchObject({ type: "downloading", paused: false });
    c.tick(3000);
    const done = sim.view();
    expect(done.change).toBeNull();
    expect(done.jobs[0]?.model).toBe("Qwen3-VL-8B");
    expect(done.custom).toBe(true);
    expect(done.old).toMatchObject({ model: "Qwen3.5-9B-GGUF", removed: false });
  });

  it("mock.models.pause_cancel_and_failure", async () => {
    const c = clock();
    const sim = new ModelsSim("standard", "standard", c.now);
    await sim.change("answering", "Qwen3-VL-8B", true);
    c.tick(1000);
    const paused = await sim.pause();
    c.tick(10_000);
    expect(sim.view().change?.phase).toEqual(paused.change?.phase);
    expect(paused.change?.phase).toMatchObject({ paused: true });
    await sim.resume();
    expect((await sim.cancel()).change).toBeNull();
    expect(sim.view().jobs[0]?.model).toBe("Qwen3.5-9B-GGUF");
    const failed = new ModelsSim("standard", "attention").view();
    expect(failed.attention).toBe(true);
    expect(failed.change?.phase.type).toBe("failed");
  });

  it("mock.models.reindex_runs_then_swaps: the finding model changes only when the re-read is whole", async () => {
    const c = clock();
    const sim = new ModelsSim("standard", "standard", c.now);
    await sim.change("finding", "nomic-embed-text-v2-moe", true);
    c.tick(5000);
    const view = sim.view();
    expect(view.reindex?.phase).toBe("running");
    expect(view.jobs[3]?.model).toBe("Qwen3-Embedding-0.6B-GGUF");
    c.tick(5000);
    const done = sim.view();
    expect(done.reindex?.phase).toBe("done");
    expect(done.jobs[3]?.model).toBe("nomic-embed-text-v2-moe");
    const cancelled = new ModelsSim("standard", "reindex");
    expect((await cancelled.cancelReindex()).reindex?.phase).toBe("cancelled");
    expect(cancelled.view().jobs[3]?.model).toBe("Qwen3-Embedding-0.6B-GGUF");
  });

  it("mock.models.old_keep_remove_reset", async () => {
    const sim = new ModelsSim("standard", "oldKeep");
    expect(sim.view().old).toMatchObject({ removed: false });
    expect((await sim.removeOld()).old).toMatchObject({ removed: true });
    expect((await sim.reset()).custom).toBe(false);
    const kept = new ModelsSim("standard", "oldKeep");
    expect((await kept.keepOld()).old).toBeNull();
  });
});
