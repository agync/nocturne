import { screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { RemoteView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";
import { indicatorFor } from "./Rail";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const rail = async (scenario?: string) => {
  const engine = createMockEngine({ remote: { auto: false, scenario } });
  renderApp({ hash: "#/today", engine });
  const nav = await screen.findByRole("navigation", { name: "Main" });
  await screen.findByRole("region", { name: "Nocturne status" });
  await engine.getRemote();
  return { nav, engine };
};

describe("rail", () => {
  it("indicator_states: says nothing when off, and otherwise one plain line that opens the page", async () => {
    let { nav } = await rail("off");
    expect(
      within(nav).queryByRole("link", { name: /Away from home|using Nocturne|new address/ }),
    ).toBeNull();
    for (const [scenario, text] of [
      ["on", "Away from home is on"],
      ["inUse", "Pixel is using Nocturne"],
      ["relink", "Your phones need the new address"],
      ["problem", "Away from home needs you"],
      ["signIn", "Away from home needs you"],
    ] as const) {
      document.body.innerHTML = "";
      ({ nav } = await rail(scenario));
      const link = await within(nav).findByRole("link", { name: text });
      expect(link, scenario).toHaveAttribute("href", "#/settings/away");
    }
  });

  it("indicator_states: announces only what needs the person", async () => {
    for (const [scenario, text, announced] of [
      ["on", "Away from home is on", false],
      ["inUse", "Pixel is using Nocturne", false],
      ["relink", "Your phones need the new address", true],
      ["problem", "Away from home needs you", true],
    ] as const) {
      document.body.innerHTML = "";
      const { nav } = await rail(scenario);
      const link = await within(nav).findByRole("link", { name: text });
      expect(link.closest("[aria-live=polite]"), scenario).not.toBeNull();
      expect(link.closest("[aria-live=off]") !== null, scenario).toBe(!announced);
    }
  });

  it("indicator_states: names two phones by count and says nothing while setting up", () => {
    const base = {
      state: { type: "on", address: "a", machineName: null },
      devices: [
        { id: "1", name: "A", pairedAt: "", lastUsedAt: null, active: true, needsAddress: false },
        { id: "2", name: "B", pairedAt: "", lastUsedAt: null, active: true, needsAddress: false },
      ],
    } as unknown as RemoteView;
    expect(indicatorFor(base)?.text).toBe("2 phones are using Nocturne");
    expect(indicatorFor({ ...base, state: { type: "starting" } })).toBeNull();
    expect(indicatorFor(undefined)).toBeNull();
  });

  it("sent_to_phones_only_when_used: the status card adds the line only once something was sent", async () => {
    let { nav } = await rail("on");
    expect(await within(nav).findByText("Sent to your phones")).toBeInTheDocument();
    expect(within(nav).getByText("3.2 MB")).toBeInTheDocument();
    for (const scenario of ["onAlone", "off", "offBefore"]) {
      document.body.innerHTML = "";
      ({ nav } = await rail(scenario));
      expect(within(nav).queryByText("Sent to your phones"), scenario).toBeNull();
    }
  });
});
