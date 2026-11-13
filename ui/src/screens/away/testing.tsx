import { screen } from "@testing-library/react";
import { createMockEngine, type MockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

/** A mock computer whose scripted steps move only when a test says so. */
export function manualEngine(scenario?: string): MockEngine {
  return createMockEngine({ remote: { auto: false, scenario } });
}

/** Opens a screen of Away from home on a mock that starts in the named state. */
export async function openAway(
  path: string,
  scenario?: string,
  heading: string | RegExp = "Away from home",
  engine: MockEngine = manualEngine(scenario),
) {
  setViewport(1280);
  const view = renderApp({ hash: `#${path}`, engine });
  await screen.findByRole("heading", { level: 1, name: heading });
  return { ...view, engine };
}

/** Pretends the web view belongs to another kind of computer. */
export function pretendComputer(kind: "mac" | "windows" | "linux"): void {
  const agent = {
    mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
    windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
  }[kind];
  Object.defineProperty(navigator, "userAgent", { configurable: true, get: () => agent });
}
