import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { copyDeck, t } from "../../copy/t";
import { fakePlatform, renderApp, setViewport } from "../../test/render";
import { CHIP_IDS, CHIPS, KEYS } from "./registry";

const words = (id: keyof typeof CHIPS) => t(CHIPS[id].copy);

describe("a chip names its control in the control's own words", () => {
  beforeEach(() => setViewport(1280));
  afterEach(() => {
    window.location.hash = "";
  });

  it("finds Voice, Away from home, Online answers and Show advanced details on Settings", async () => {
    renderApp({ hash: "#/settings", platform: fakePlatform() });
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    await screen.findByRole("combobox", { name: `${words("voice")}, name` });
    expect(screen.getByText(words("awayFromHome"), { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText(words("onlineAnswers"), { selector: "span" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: words("showAdvancedDetails") })).toBeInTheDocument();
  });

  it("says the same words wherever the same control appears", () => {
    expect(words("seeWhatWasSent")).toBe(t("online.link.sent"));
    expect(words("seeWhatWasSent")).toBe(t("privacy.online.seeSent"));
    expect(words("newCodeToScan")).toBe(t("phone.scan.title.relink"));
    expect(words("restoreFromBackup")).toBe(t("restore.title"));
    expect(words("restoreFromBackup")).toBe(t("screen.restore.title"));
    expect(words("addAPhone")).toBe(t("screen.away.add.title"));
    expect(words("showAdvancedDetails")).toBe(t("online.link.advanced"));
    expect(words("showAdvancedDetails")).toBe(t("away.advanced.show"));
  });
});

describe("the chip registry and the deck", () => {
  const named = (id: string) =>
    Object.values(copyDeck).some((text) => new RegExp(`\\{${id}\\}`).test(text));

  it("has no chip that no sentence uses", () => {
    expect(CHIP_IDS.filter((id) => !named(id))).toEqual([]);
  });

  it("has no key that no sentence uses", () => {
    expect(Object.keys(KEYS).filter((id) => !named(id))).toEqual([]);
  });

  it("puts at most two chips in a sentence, or three in a path joined by then or under", () => {
    const over: string[] = [];
    for (const [id, text] of Object.entries(copyDeck)) {
      for (const sentence of text.split(/(?<=[.!?])\s+/)) {
        const chips = CHIP_IDS.filter((chip) => sentence.includes(`{${chip}}`)).length;
        const path = /\b(then|under)\b/.test(sentence);
        if (chips > (path ? 3 : 2)) over.push(id);
      }
    }
    expect(over).toEqual([]);
  });

  it("never writes a chip inside a heading or a button label", () => {
    const labelled = Object.entries(copyDeck)
      .filter(([id]) => /(\.title|\.label|\.heading|\.button)$/.test(id))
      .filter(([, text]) => CHIP_IDS.some((chip) => text.includes(`{${chip}}`)))
      .map(([id]) => id);
    expect(labelled).toEqual([]);
  });
});
