import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { type ModelsScenario, ModelsSim } from "../../engine/mock/models";

/**
 * A mock engine whose models run on a simulation of this scenario, with each command wired to
 * what it names. The shared mock engine sends them all to one `step`, which cannot tell pause
 * from cancel, so tests of those wire them here.
 */
export function modelsEngine(
  scenario: ModelsScenario = "standard",
  now: () => number = () => Date.now(),
): { engine: MockEngine; sim: ModelsSim } {
  const engine = createMockEngine();
  const sim = new ModelsSim("standard", scenario, now);
  Object.assign(engine, {
    models: sim,
    pauseModelDownload: () => sim.pause(),
    resumeModelDownload: () => sim.resume(),
    cancelModelChange: () => sim.cancel(),
    cancelReindex: () => sim.cancelReindex(),
    keepOldModel: () => sim.keepOld(),
    removeOldModel: () => sim.removeOld(),
    resetModelsToTier: () => sim.reset(),
  });
  return { engine, sim };
}
