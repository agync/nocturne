import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { type ModelsScenario, ModelsSim } from "../../engine/mock/models";

/** A mock engine whose models run on a simulation of this scenario, on a clock the test can hold. */
export function modelsEngine(
  scenario: ModelsScenario = "standard",
  now: () => number = () => Date.now(),
): { engine: MockEngine; sim: ModelsSim } {
  const engine = createMockEngine();
  const sim = new ModelsSim("standard", scenario, now);
  Object.assign(engine, { models: sim });
  return { engine, sim };
}
