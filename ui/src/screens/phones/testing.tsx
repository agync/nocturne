import { screen } from "@testing-library/react";
import { createMockEngine } from "../../engine/mock/engine";
import { withPhones } from "../../engine/mock/phones";
import { renderApp, setViewport } from "../../test/render";

export const NOW = "2026-11-13T09:00:00Z";

/** The mock computer with Sam's phone (the app) and Ana's (the browser), on a fixed day. */
export function phonesEngine(options: { empty?: boolean } = {}) {
  return withPhones(createMockEngine(), NOW, options);
}

export async function openPhones(path: string, heading: string | RegExp, engine = phonesEngine()) {
  setViewport(1280);
  const view = renderApp({ hash: `#${path}`, engine });
  await screen.findByRole("heading", { level: 1, name: heading });
  return { ...view, engine };
}
