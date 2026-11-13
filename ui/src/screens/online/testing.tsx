import { screen } from "@testing-library/react";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import type { OnlineScenario } from "../../engine/mock/online";
import { renderApp, setViewport } from "../../test/render";

/** A mock computer that starts in the named Online answers state, with checks that end at once. */
export function onlineEngine(scenario: OnlineScenario = "off"): MockEngine {
  const engine = createMockEngine();
  engine.online.setScenario(scenario);
  engine.online.setCheckMs(0);
  return engine;
}

/** Opens a screen of Online answers on a mock that starts in the named state. */
export async function openOnline(
  path: string,
  scenario: OnlineScenario = "off",
  heading: string | RegExp = "Online answers",
  engine: MockEngine = onlineEngine(scenario),
) {
  setViewport(1280);
  const view = renderApp({ hash: `#${path}`, engine });
  await screen.findByRole("heading", { level: 1, name: heading });
  return { ...view, engine };
}
